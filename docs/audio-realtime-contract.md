# Preview audio real-time contract

This document is normative for the CPAL output data callback in `punks-audio`.
The preview player is intentionally still a simple one-file audition path. This
contract is the boundary future audio work must preserve; it is not a DAW engine
design.

## Ownership and thread boundaries

| Actor | Owns / performs | Must not do |
|---|---|---|
| UI/application control | Selection, playback state, cache, handoff publication and retirement, stream recovery, user-visible errors | Decode or file-sized I/O in the draw loop |
| Decode worker | File I/O, decode, channel adaptation, resampling, prepared audio creation | Touch CPAL callback state directly |
| Analysis/peaks workers | Waveform and analysis work | Touch CPAL callback state directly |
| Audio control path (`play`, `poll`, `stop`, `seek_to`) | Atomic control updates, bounded request/result mailboxes, buffer ownership, CPAL stream lifecycle | Put obsolete work into an unbounded queue |
| CPAL data callback | Read the published immutable sample buffer, advance cursor, apply gain, write output, acknowledge callback use | Everything in the contract below |

## Callback object audit

This is the complete callback call-graph inventory. “RT safe” means the callback
may access the object in the listed way; it does not transfer ownership.

| Object touched by the data callback | Classification | Allowed callback operation | Ownership / reason |
|---|---|---|---|
| `SharedState::published` | RT safe | Acquire-load a raw pointer | The control side publishes a heap-stable `PublishedAudio` and retains its `Box` until the callback-user acknowledgement reaches zero. |
| `PublishedAudio::audio.samples` | RT safe | Borrow and read immutable `f32` samples | The `Arc<PreparedAudio>` is cloned and dropped only by control-side code; the callback never changes its reference count. |
| `PublishedAudio::cursor` | RT safe | Relaxed atomic load/store | Only the callback advances the cursor for that published generation. |
| `PublishedAudio::playing` | RT safe | Acquire/relaxed atomic load/store | Control-side stop/seek publication uses atomics; the callback only changes it when the bounded buffer is exhausted. |
| `SharedState::volume` | RT safe | Relaxed atomic load | The control path stores a prepared `f32` bit pattern; the callback applies one multiplication per output sample. |
| `SharedState::callback_users` | RT safe | Atomic increment/decrement | This is the lifetime acknowledgement, not a lock. It prevents control-side retirement from destroying a borrowed buffer. |
| `SharedState::acknowledged_generation` | RT safe | Release atomic store | Diagnostic/progress acknowledgement only; it never owns or drops data. |
| `SharedState::stream_failed` | Forbidden to data callback | Not accessed by the data callback | Only CPAL's separate stream-error callback stores it; `poll()` consumes it and performs recovery. |
| `PlaybackEngine`, `RequestSlot`, `LruCache`, `cpal::Stream` | Forbidden | No access | These are control-side owners and lifecycle objects. Their locks, channels, allocations, I/O, and device operations stay off the data callback. |

The audit was performed against the callback body and its direct callees:
atomic operations, slice reads/writes, `f32` multiplication, and `fill` are the
only operations in the production path. The test-only allocator probe asserts
that the callback performs neither allocation nor deallocation; lock freedom is
also a call-graph property because no lock-bearing object is reachable from the
callback's arguments.

## Audit record

The pre-rewrite callback was inspected before changing the handoff. It used
`SharedState::samples: RwLock<Vec<f32>>` and called `try_read()` on every audio
buffer. That avoided waiting in the common path but was still lock acquisition,
so it did not satisfy this contract. The old stream-error closure logged
directly, and decode results used an unbounded channel; the decode worker also
had no shutdown signal. Those were control-boundary violations or lifecycle
risks, not callback work to preserve.

After the rewrite, the callback path is statically limited to the table above.
The test-only allocator probe found zero allocations, reallocations, or
deallocations during a callback invocation. The lock claim is verified by the
complete callback call-graph audit: no `Mutex`, `RwLock`, channel, logger, or
allocation-bearing owner is reachable from `audio_callback`. A real-device
dropout measurement requires an output device and is reported separately from
these deterministic tests.

## Callback invariant

The data callback must perform no:

- `Mutex`/`RwLock` acquisition or lock-like retry;
- blocking operation, waiting, sleeping, or cross-thread rendezvous;
- heap allocation or deallocation;
- `Arc::clone`, `Arc` drop, or other reference-count mutation;
- filesystem or other I/O;
- decoding, resampling, analysis, waveform work, or metadata work;
- logging, formatting, error construction, or string handling.

It may only read already-prepared immutable samples, read bounded atomic control
state, advance the playback cursor, apply trivial gain, update bounded atomic
acknowledgement state, and write the output slice. The callback call graph must
remain small enough for a code review to audit completely.

The CPAL stream error callback is a separate fault signal path. It only sets an
atomic failure flag. It does not log, rebuild the stream, allocate a message, or
attempt recovery. `poll()` performs recovery on the control thread.

## Published buffer handoff

The callback reads a raw pointer to a published `PublishedAudio` object whose
sample data and metadata are immutable. Its cursor and playing flag are
per-buffer atomics, so an old callback cannot overwrite the state of a newly
published buffer. The pointer is published with release ordering and loaded
with acquire ordering. The control side owns the heap allocation in a `Box`;
the callback only borrows it.
The control side never retires that owner while a callback could still hold the
pointer:

1. The callback increments `callback_users` before loading the pointer and
   decrements it after its final sample read.
2. Control publication swaps the pointer only after the new owner is live.
3. The superseded owner becomes `retired` if a callback is in flight. Control
   drops it only after `callback_users == 0`.
4. A newer prepared result occupies the single `pending` slot. It replaces the
   previous pending result; it never creates another owned buffer.
5. The callback stores the generation it finished using after its sample reads.
   This acknowledgement is diagnostic and part of the handoff proof; the
   in-flight count is the lifetime guard that makes retirement safe.

The generation acknowledgement must never be implemented by cloning an `Arc` or
by dropping an owner. Both reference-count operations remain control-side.

At most one active, one retired, and one pending handoff buffer are owned by the
control side. Stream teardown first stops the stream and waits for in-flight
callbacks to acknowledge, then clears the published pointer and destroys the
buffers.

## Latest-wins requests

Selection requests use a one-slot latest-wins mailbox. There is at most one
queued request and one decode in progress. Decode results use a bounded
single-capacity channel; stale generations are discarded by `poll()`. A burst
of selections therefore cannot accumulate obsolete playback commands or an
unbounded result backlog.

## Device and stream failure policy

- No default output device or an unsupported output sample format is a clear
  construction error.
- A runtime stream error stops playback and is observed by `poll()`.
- One best-effort default-device rebuild is attempted per failure episode.
- A successful rebuild marks the stream healthy; a later distinct stream-error
  callback is a new episode. A failed rebuild leaves the engine stopped until
  the next explicit Play, so `poll()` cannot become a rebuild loop.
- A successful rebuild re-decodes the current file for the current device rate
  and channel count; old-format cached buffers are discarded.
- A failed rebuild stops playback and exposes a recoverable error. A later
  explicit Play is the next retry; there is no rebuild loop in `poll()`.
- Dropping `PlaybackEngine` stops the stream, waits for callback acknowledgement,
  disconnects the bounded result channel, signals worker shutdown, and joins the
  decode worker.

## Verification

The pure callback tests exercise volume, short-buffer zero fill, generation
acknowledgement, buffer replacement, and channel adaptation without an audio
device. The complete callback call graph is reviewed for forbidden operations.
Any future callback change must extend those tests and re-audit this document.
