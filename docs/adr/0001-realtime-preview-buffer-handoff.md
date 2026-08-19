# Immutable prepared buffer handoff for preview audio

The preview player publishes immutable, device-compatible prepared audio to the CPAL callback through a bounded handoff with active, retired, and latest-pending states. The callback never clones or drops an `Arc`, acquires a lock, blocks, allocates, decodes, performs I/O, or logs; the control side owns all publication and retirement after callback acknowledgement. This was chosen over an `RwLock` because an audio callback must not contend for a lock, and over an unbounded command/result queue because rapid audition is latest-wins and obsolete work must not accumulate.

## Consequences

- Callback-visible ownership is borrowed from already-published control-owned storage; refcount changes and destruction stay off the callback thread.
- A stalled callback can delay retirement, but the handoff remains bounded and the control side replaces only its one pending candidate.
- If the output stream disappears, control-side stream teardown makes all callback-visible buffers safe to retire before a later rebuild or explicit play retry.
