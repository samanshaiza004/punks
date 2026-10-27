# SQLite compatibility and repair

Punks supports the historical version 1–6 schemas from Punks commit
`891768530f0b9b3c725b5985f415c1489a5eb75a`. The checked-in test fixtures are
independent copies of those original SQL definitions. The starting foundation
checkout incorrectly used the v6 table rebuild as v1 and omitted v2–v6.

Fresh creation runs the ordered migrations in one immediate transaction.
An existing database's version is reread after obtaining the transaction lock.
Current v6 databases take only a read transaction to validate their required
shape; reopening does not change schema or user_version. Foreign keys are
enabled on each connection. WAL is enabled after schema validation/migration;
only SQLite busy/locked failures are retried. Opening a library does not create
a missing database. Errors identify the database path and failed migration.

The preservation policy is explicit:

- v1 assets/UUIDs, tags, assignments, custom fields and values remain in place.
- v2 analyzer-specific jobs and numeric results are retained in
  `analysis_jobs_v2_archive` and `audio_analysis_v2_archive`. The current job
  becomes pending at pipeline version 0 so the current pipeline can recompute
  it. For overlapping metric names, the current result uses the latest
  computed_at, with analyzer name as a deterministic tie break. Every original
  result remains in the archive, including conflicts.
- v3 numeric results are copied into v4 real_value with timestamps intact.
  Typed real/text/blob results introduced in v4 remain unchanged.
- v5 user overrides are rebuilt with the v6 explicit-absence flag. Values,
  provenance and timestamps survive. Existing v6 absent overrides survive.
- All migration steps, schema version changes and data conversion commit
  together. A failed later step rolls back the earlier steps.

Unknown future versions, nonempty unversioned databases, incompatible required
columns and conflicting migration tables are rejected. In particular, an
existing `fact_overrides_v5` is not discarded: Punks leaves both sources intact
and reports the migration failure for manual investigation. No supported
history justifies guessing how an arbitrary partial migration should merge.
Make a backup before manual recovery; the application never deletes a user
library to make an upgrade succeed.

Pending job claims now select and update under one immediate transaction, so
competing connections cannot claim the same job. Ordinary same-version enqueue
does not reset a running job. `reset_running_jobs` remains an explicit recovery
operation; background workers do not steal another connection's running work.
A modified file schedules reanalysis without deleting user metadata or the
previous result before replacement.

Scans propagate directory/entry metadata failures rather than interpreting a
partial traversal as missing assets. Cancellation rolls back reconciliation.
Renames and moves retain identity and metadata; missing files retain rows and
user data. The existing size plus bounded first/last-content fingerprint remains
the reconciliation heuristic; it is not a full-file uniqueness guarantee.

Validation: `cargo test -p punks-library`. The suite covers fresh creation,
all six historical fixture schemas, concurrent migration/claiming, reopen,
rollback, rejected unknown/remnant schemas, data preservation, renames/moves,
missing/reappearing files, duplicates, hashing failure and cancellation. All
fixtures are temporary; no production libraries are opened by these tests.
