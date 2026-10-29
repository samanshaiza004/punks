# Native folder-dialog investigation

This is a controlled diagnostic procedure for the intermittent Windows folder
picker termination. The available dumps show a COM access violation in
`KERNELBASE!QISearch`, but that symbol alone does not identify the caller or
the failing stage. Do not infer a fault owner from that frame.

## Breadcrumbs

Build and run with `--diagnostic-events`. Alicorn writes a flushed JSONL event
file below its SDL preference directory and prints the exact path as
`alicorn_diagnostics events ...`. Events contain schema version, UTC and
monotonic timestamps, thread ID, source, fixed event name, and request ID. They
omit paths and payload text. Alicorn records dialog receipt, invocation,
SDL-call return, callback entry/status/queue/completion, result dispatch, and
shutdown. Punks records directory-open request, dialog result, command
submission, and the corresponding result publication.

Punks assigns one request ID to the dialog and the resulting `open` command.
`directory.enumeration.started` is recorded immediately before the command is
sent across the Caliber boundary; `directory.enumeration.completed` or
`.failed` is recorded when the backend publishes the matching request. Thus
these events bracket the backend operation without making Alicorn depend on
Caliber or the filesystem.

The debug build emits `target/windows-x86_64/debug/punks.pdb` beside
`punks.exe`. Keep both files from the same build when reproducing a crash.
The Alicorn fixture build similarly emits `out/alicorn_sdl_gpu.pdb`.

## Controlled test matrix

Use the same diagnostic build for each run. The isolated Alicorn fixture
contains no Punks or Caliber code:

```powershell
cd C:\Users\saman\dev\alicorn
.\tools\native_sdl_gpu.ps1 -DialogFixture -DialogInitialLocation none -DiagnosticEvents
.\tools\native_sdl_gpu.ps1 -DialogFixture -DialogInitialLocation ordinary -DiagnosticEvents
.\tools\native_sdl_gpu.ps1 -DialogFixture -DialogInitialLocation extended -DiagnosticEvents
```

For each initial-location mode, cancel once, select a directory once, reopen
the picker, and close the application while the picker is outstanding. Record
whether termination occurs before the picker appears, while browsing, on
selection/cancel, or after callback dispatch. Repeat the same cases in Punks:

```powershell
cd C:\Users\saman\dev\punks
& .\target\windows-x86_64\debug\punks.exe --no-audio --folder 'C:\Samples' --diagnostic-events --dialog-initial=none
& .\target\windows-x86_64\debug\punks.exe --no-audio --folder 'C:\Samples' --diagnostic-events --dialog-initial=ordinary
& .\target\windows-x86_64\debug\punks.exe --no-audio --folder 'C:\Samples' --diagnostic-events --dialog-initial=extended
```

Replace `C:\Samples` with an existing local directory. Run the picker manually
without screen capture or UI Automation attached for the control pass. If
practical, repeat with third-party Shell extensions and accessibility tools
disabled as a separate variable. Do not share a full crash dump without
reviewing it for private process memory.

## Dump-symbol status

The two previously collected dumps are in `%LOCALAPPDATA%\CrashDumps` and were
written on October 28, 2026. The available `punks-debug.pdb` was produced later
that evening, so it has not been established as a match for either dump. This
machine currently has no `cdb.exe` or `windbg.exe` available on `PATH`, and the
older dumps have not been attributed to a specific dialog test. A symbolized
stack and faulting-instruction context therefore remain outstanding. Save the
matching executable and PDB beside each new reproduction before analyzing it
with WinDbg; inspect the requested IID pointer and the `QITAB` argument
separately.

## Interpreting the trail

- Missing `dialog.invoke` means the request was rejected or failed during
  request preparation.
- `dialog.invoke` without `dialog.invoke_return` narrows the fault to the SDL
  call boundary or its synchronous native path.
- `dialog.callback.enter` without `dialog.callback.completed` narrows it to
  callback processing or queue/wake handling.
- `dialog.result.dispatched` without `dialog.result.dispatch_completed` narrows
  it to Punks' UI-thread result handler.
- A completed callback/dispatch followed by a Punks enumeration start without
  completion shifts attention to the Caliber/backend boundary.

These are phase boundaries, not proof that the last logged subsystem caused a
crash. Preserve the JSONL file, exact executable/PDB revisions, OS build, SDL
version, initial-location mode, action taken, and whether automation tools were
attached for each reproduction.
