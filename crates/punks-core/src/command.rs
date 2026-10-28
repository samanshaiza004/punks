//! A Command system unifying the app's writes so they can be undone.
//! Two kinds exist today and both go through the same [`Command`] trait:
//! SQLite writes (tags, overrides — `punks_library::Library`) and embedded
//! file metadata writes (`punks_audio::metadata`, e.g. the bext
//! Description). [`WriteMetadataCommand`] shows the "unify" part
//! concretely: one command does *both* — the embedded write and the
//! library's cache sync — as a single undoable step, exactly the shape
//! `SampleBrowser::set_description` already had; this just makes it
//! reversible. A third kind — project-file writes — was named in the
//! original plan as future work; nothing here forecloses it (a command
//! just needs a way to read/write whatever that file turns out to be), but
//! there's no project-file feature yet, so no command exists for it.
//!
//! [`BatchCommand`] groups several commands into one undo step, which is
//! what makes "batch tag" a single "undo last tag assignment" instead of N.
//! It also backs the open-span *transaction* API (`begin_transaction` /
//! `commit_transaction` on [`UndoStack`]): commands recorded between those
//! calls fold into one `BatchCommand` — the shape `SampleBrowser`'s batch-tag
//! paths use so N mutations become one undo step and one cache reload.
//!
//! An in-memory [`UndoStack`] per library (see `LibraryContext::undo_stack`),
//! linear undo **and redo** (a fresh command clears the redo branch), no
//! persistence across restarts. Scope is intentionally narrow: tag
//! assign/unassign and override set/mark-absent/clear/description writes
//! are undoable; tag *creation/deletion* and library creation/deletion are
//! not — those are structural/administrative actions where "undo" would
//! mean re-deleting or re-creating rows that may have gained new
//! dependents since (e.g. deleting a newly-created tag that a second
//! action already assigned elsewhere), which is a correctness question
//! this pass doesn't need to answer. Extend `Command` impls here, not the
//! trait, if that scope grows.

use std::path::PathBuf;

use punks_audio::{Backend, Field, Metadata, MetadataBackend, MetadataError};
use punks_library::{Fact, Library, LibraryError};

/// What a [`Command`] executes/undoes against. Just the library handle —
/// every write this module wraps already only needed that (metadata
/// commands read/write the file directly through `Backend::for_path`,
/// which needs nothing but the path each command already carries).
pub struct CommandContext<'a> {
    pub lib: &'a mut Library,
}

#[derive(Debug)]
pub enum CommandError {
    Library(LibraryError),
    Metadata(MetadataError),
}

impl std::fmt::Display for CommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CommandError::Library(e) => write!(f, "{e}"),
            CommandError::Metadata(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for CommandError {}

/// One undoable write. `execute`/`undo` take `&mut self` because most
/// impls capture "before" state the first time `execute` runs (there's no
/// separate prepare step) so `undo` has something to restore — see each
/// impl's doc comment for exactly what it captures.
pub trait Command {
    fn execute(&mut self, ctx: &mut CommandContext) -> Result<(), CommandError>;
    fn undo(&mut self, ctx: &mut CommandContext) -> Result<(), CommandError>;
    /// One-line label for an "Undo: {description}" UI affordance.
    fn description(&self) -> String;
}

/// Assign `tag_id` to `path`. Undo unassigns it — safe and exact regardless
/// of whether the tag existed before this command (assignment is a pure
/// join-table row, not the tag itself).
pub struct AssignTagCommand {
    pub path: PathBuf,
    pub tag_id: i64,
    pub tag_name: String,
}

impl Command for AssignTagCommand {
    fn execute(&mut self, ctx: &mut CommandContext) -> Result<(), CommandError> {
        ctx.lib
            .assign_tag(&self.path, self.tag_id)
            .map_err(CommandError::Library)
    }
    fn undo(&mut self, ctx: &mut CommandContext) -> Result<(), CommandError> {
        ctx.lib
            .remove_tag(&self.path, self.tag_id)
            .map_err(CommandError::Library)
    }
    fn description(&self) -> String {
        format!("assign tag \"{}\"", self.tag_name)
    }
}

/// Remove `tag_id` from `path`. Undo re-assigns it — the exact inverse of
/// [`AssignTagCommand`].
pub struct UnassignTagCommand {
    pub path: PathBuf,
    pub tag_id: i64,
    pub tag_name: String,
}

impl Command for UnassignTagCommand {
    fn execute(&mut self, ctx: &mut CommandContext) -> Result<(), CommandError> {
        ctx.lib
            .remove_tag(&self.path, self.tag_id)
            .map_err(CommandError::Library)
    }
    fn undo(&mut self, ctx: &mut CommandContext) -> Result<(), CommandError> {
        ctx.lib
            .assign_tag(&self.path, self.tag_id)
            .map_err(CommandError::Library)
    }
    fn description(&self) -> String {
        format!("remove tag \"{}\"", self.tag_name)
    }
}

/// What to do to a fact's override row (mirrors the tri-state contract
/// `Library::set_override`/`mark_absent`/`clear_override` already have).
pub enum OverrideAction {
    /// Correct the metric to `Fact`.
    Set(Fact),
    /// Explicitly mark the metric absent (hides the detected guess).
    MarkAbsent,
    /// Remove the override row entirely (fall back to detected).
    Clear,
}

/// Apply an [`OverrideAction`] to `metric` on `path`. Captures the prior
/// override row (no row / marked absent / a value) the first time it
/// executes, so undo restores exactly that prior state rather than always
/// falling back to "clear" — a `Set` that corrected an *existing* override
/// must undo back to that override, not wipe it.
pub struct SetOverrideCommand {
    pub path: PathBuf,
    pub metric: String,
    pub action: OverrideAction,
    /// The override row's state before this command first ran: no row /
    /// `Some(None)` marked absent / `Some(Some(f))` a prior value. Only
    /// meaningful once `captured` is true — kept as a separate flag rather
    /// than folding "not captured yet" into this via a third `Option`
    /// layer, which would make every match here three-deep for no reason.
    prior: Option<Fact>,
    has_prior_row: bool,
    captured: bool,
}

impl SetOverrideCommand {
    pub fn new(path: PathBuf, metric: impl Into<String>, action: OverrideAction) -> Self {
        SetOverrideCommand {
            path,
            metric: metric.into(),
            action,
            prior: None,
            has_prior_row: false,
            captured: false,
        }
    }

    fn read_current(&self, lib: &Library) -> Result<Option<Option<Fact>>, LibraryError> {
        let rows = lib.overrides(&self.path)?;
        Ok(rows
            .into_iter()
            .find(|(m, _)| *m == self.metric)
            .map(|(_, v)| v))
    }

    fn apply(
        &self,
        ctx: &mut CommandContext,
        has_row: bool,
        value: &Option<Fact>,
    ) -> Result<(), CommandError> {
        if !has_row {
            ctx.lib.clear_override(&self.path, &self.metric)
        } else {
            match value {
                None => ctx.lib.mark_absent(&self.path, &self.metric),
                Some(f) => ctx.lib.set_override(&self.path, &self.metric, f),
            }
        }
        .map_err(CommandError::Library)
    }
}

impl Command for SetOverrideCommand {
    fn execute(&mut self, ctx: &mut CommandContext) -> Result<(), CommandError> {
        if !self.captured {
            match self.read_current(ctx.lib).map_err(CommandError::Library)? {
                None => {
                    self.has_prior_row = false;
                    self.prior = None;
                }
                Some(value) => {
                    self.has_prior_row = true;
                    self.prior = value;
                }
            }
            self.captured = true;
        }
        match &self.action {
            OverrideAction::Set(f) => self.apply(ctx, true, &Some(f.clone())),
            OverrideAction::MarkAbsent => self.apply(ctx, true, &None),
            OverrideAction::Clear => self.apply(ctx, false, &None),
        }
    }
    fn undo(&mut self, ctx: &mut CommandContext) -> Result<(), CommandError> {
        let has_row = self.has_prior_row;
        let prior = self.prior.clone();
        self.apply(ctx, has_row, &prior)
    }
    fn description(&self) -> String {
        match &self.action {
            OverrideAction::Set(_) => format!("set {}", self.metric),
            OverrideAction::MarkAbsent => format!("mark {} absent", self.metric),
            OverrideAction::Clear => format!("clear {} override", self.metric),
        }
    }
}

/// Write a whole [`Metadata`] into `path`'s embedded metadata *and* sync the
/// library's cached copies — the "unify SQLite + embedded metadata" case the
/// module doc calls out, generalized from Description to every field the
/// panel edits (Description/Keywords/Category/Creator; the backend writes
/// only those its format supports). Captures the prior full `Metadata` the
/// first time it executes; undo writes that back through the same backend and
/// re-syncs the cache, so the cache never disagrees with the file even after
/// an undo. One "edit metadata" undo step per panel Save.
///
/// The backend only writes supplied values. Undoing an edit that adds a field
/// restores an empty value instead of removing the tag.
pub struct WriteMetadataCommand {
    pub path: PathBuf,
    pub new: Metadata,
    prior: Option<Metadata>,
}

impl WriteMetadataCommand {
    pub fn new(path: PathBuf, new: Metadata) -> Self {
        WriteMetadataCommand {
            path,
            new,
            prior: None,
        }
    }

    /// Write `m` through the backend, then mirror the scalar fields into the
    /// library cache (normalized exactly as the backend stored them, so the
    /// cache can't disagree with the file — that's what keeps the health
    /// validator from flagging a punks-originated edit as drift). Keywords is
    /// a `Vec` with no scalar `Fact`, so it isn't cached.
    fn write_and_cache(&self, ctx: &mut CommandContext, m: &Metadata) -> Result<(), CommandError> {
        let backend = Backend::for_path(&self.path);
        backend
            .write(&self.path, m)
            .map_err(CommandError::Metadata)?;
        let cache =
            |ctx: &mut CommandContext, metric: &str, field: Field, value: &Option<String>| {
                if let Some(v) = value {
                    let normalized = backend.normalize(field, v);
                    ctx.lib
                        .set_metadata_text(&self.path, metric, &normalized)
                        .map_err(CommandError::Library)
                } else {
                    Ok(())
                }
            };
        cache(ctx, "description", Field::Description, &m.description)?;
        cache(ctx, "category", Field::Category, &m.category)?;
        cache(ctx, "creator", Field::Creator, &m.creator)?;
        Ok(())
    }
}

impl Command for WriteMetadataCommand {
    fn execute(&mut self, ctx: &mut CommandContext) -> Result<(), CommandError> {
        if self.prior.is_none() {
            let backend = Backend::for_path(&self.path);
            self.prior = Some(backend.read(&self.path).map_err(CommandError::Metadata)?);
        }
        let new = self.new.clone();
        self.write_and_cache(ctx, &new)
    }
    fn undo(&mut self, ctx: &mut CommandContext) -> Result<(), CommandError> {
        let prior = self.prior.clone().unwrap_or_default();
        self.write_and_cache(ctx, &prior)
    }
    fn description(&self) -> String {
        "edit metadata".to_string()
    }
}

/// Groups several commands into one undo step. `execute`/`undo` attempt
/// every sub-command even if one fails (matching how the pre-Command batch
/// tag methods already behaved: log-and-continue, not abort-on-first-error
/// — see `SampleBrowser::assign_tag_batch`'s history), returning the first
/// error encountered, if any, so the caller can still surface it. Undo runs
/// in reverse order — the standard "undo the last thing first" composite
/// semantics.
pub struct BatchCommand {
    commands: Vec<Box<dyn Command>>,
    label: String,
}

impl BatchCommand {
    pub fn new(label: impl Into<String>, commands: Vec<Box<dyn Command>>) -> Self {
        BatchCommand {
            commands,
            label: label.into(),
        }
    }
}

impl Command for BatchCommand {
    fn execute(&mut self, ctx: &mut CommandContext) -> Result<(), CommandError> {
        let mut first_err = None;
        for cmd in &mut self.commands {
            if let Err(e) = cmd.execute(ctx) {
                first_err.get_or_insert(e);
            }
        }
        first_err.map_or(Ok(()), Err)
    }
    fn undo(&mut self, ctx: &mut CommandContext) -> Result<(), CommandError> {
        let mut first_err = None;
        for cmd in self.commands.iter_mut().rev() {
            if let Err(e) = cmd.undo(ctx) {
                first_err.get_or_insert(e);
            }
        }
        first_err.map_or(Ok(()), Err)
    }
    fn description(&self) -> String {
        self.label.clone()
    }
}

/// Upper bound on retained undo history. Guards against unbounded growth over
/// a very long session; the oldest step drops when the stack exceeds it.
const MAX_UNDO: usize = 256;

/// A library's executed-command history. Lives on `LibraryContext` (one
/// stack per library, not global) so undo always resolves the same library
/// a command was executed against, even if the user has since switched to
/// a different tab/library — no separate bookkeeping needed to track which
/// library each entry belongs to.
///
/// Linear history: `undo` moves a command from `done` to `undone`; `redo`
/// re-executes it back onto `done`; recording a *fresh* command clears
/// `undone` (a new edit invalidates the redo branch). An open transaction
/// (`pending`) accumulates commands so several mutations collapse into one
/// undo step at [`commit_transaction`](Self::commit_transaction).
#[derive(Default)]
pub struct UndoStack {
    done: Vec<Box<dyn Command>>,
    undone: Vec<Box<dyn Command>>,
    /// `Some((label, commands))` while a transaction is open; recorded
    /// commands accumulate here instead of landing on `done` individually.
    pending: Option<(String, Vec<Box<dyn Command>>)>,
}

impl UndoStack {
    /// Record an already-executed command. Inside a transaction it accumulates;
    /// otherwise it lands as a fresh undo step (which clears the redo branch and
    /// enforces the history cap).
    pub fn record(&mut self, command: Box<dyn Command>) {
        match &mut self.pending {
            Some((_, commands)) => commands.push(command),
            None => self.commit_single(command),
        }
    }

    fn commit_single(&mut self, command: Box<dyn Command>) {
        self.undone.clear();
        self.done.push(command);
        if self.done.len() > MAX_UNDO {
            self.done.remove(0);
        }
    }

    /// Open a transaction: subsequent [`record`](Self::record) calls group into
    /// one undo step until [`commit_transaction`](Self::commit_transaction).
    /// Transactions do not nest — a nested open is a programmer bug.
    pub fn begin_transaction(&mut self, label: impl Into<String>) {
        debug_assert!(
            self.pending.is_none(),
            "begin_transaction while a transaction is already open"
        );
        if self.pending.is_none() {
            self.pending = Some((label.into(), Vec::new()));
        }
    }

    pub fn in_transaction(&self) -> bool {
        self.pending.is_some()
    }

    /// Close the open transaction, folding its accumulated commands into one
    /// [`BatchCommand`] undo step. A transaction that recorded nothing pushes
    /// nothing. No-op if no transaction is open.
    pub fn commit_transaction(&mut self) {
        if let Some((label, commands)) = self.pending.take() {
            if !commands.is_empty() {
                self.commit_single(Box::new(BatchCommand::new(label, commands)));
            }
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.done.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.undone.is_empty()
    }

    /// Label for the command that [`undo`](Self::undo) would undo next.
    pub fn next_undo_description(&self) -> Option<String> {
        self.done.last().map(|c| c.description())
    }

    /// Label for the command that [`redo`](Self::redo) would re-apply next.
    pub fn next_redo_description(&self) -> Option<String> {
        self.undone.last().map(|c| c.description())
    }

    /// Pop and undo the most recently executed command, moving it onto the redo
    /// stack. `None` if there's nothing to undo.
    pub fn undo(&mut self, ctx: &mut CommandContext) -> Option<Result<(), CommandError>> {
        let mut command = self.done.pop()?;
        let result = command.undo(ctx);
        self.undone.push(command);
        Some(result)
    }

    /// Re-execute the most recently undone command, moving it back onto the undo
    /// stack. Safe to re-run: every command captures its "before" state behind a
    /// first-execute guard, so re-execute re-applies without re-capturing.
    /// `None` if there's nothing to redo.
    pub fn redo(&mut self, ctx: &mut CommandContext) -> Option<Result<(), CommandError>> {
        let mut command = self.undone.pop()?;
        let result = command.execute(ctx);
        self.done.push(command);
        Some(result)
    }
}
