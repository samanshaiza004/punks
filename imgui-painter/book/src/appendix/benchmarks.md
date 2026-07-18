# Benchmarks

## What is measured

`bindings/rust/benches/tessellation.rs` benchmarks `Session`-driven mesh
generation against `benches/handwritten.rs` — a from-scratch pure-Rust
tessellation with no FFI and no generic gradient dispatch.

Both produce the *same* look: the macOS-style panel from `painter_demo` (rounded
rect + soft shadow + linear gradient fill + 1px border). The comparison is
deliberately unfair to imgui-painter: the handwritten version has no FFI boundary
to cross and no generic dispatch to resolve.

```sh
cargo bench
```

## One measured run

| | imgui-painter (`Session`) | handwritten `ImDrawList`-style Rust |
|---|---|---|
| Source lines for this one look | ~42 | ~213 |
| Tessellation | adaptive, error-bounded | fixed 8 segments/corner |
| Shadow rings | one `add_shadow` call | hand-rolled ring loop + falloff math |
| Gradient | generic 4-mode dispatch | inlined 2-stop linear lerp only |
| Border ordering | call order | call order (same shape, rewritten) |
| Mesh size | 433 vtx / 1260 idx | 553 vtx / 1620 idx |
| Time per mesh | ~6.96 µs (6.91–7.02) | ~8.78 µs (8.56–9.05) |

## Reading this honestly

imgui-painter is both smaller and faster here, and the reason is
**adaptive tessellation**, not clever micro-optimization. The segment count per
corner is derived from each corner's actual radius against a flatness tolerance,
so a small corner does not get the segment budget of a large one. The handwritten
version spends a fixed 8 segments per corner regardless.

Fewer vertices means less to generate and less for the GPU to consume — enough to
pay for the FFI crossing the handwritten version avoids.

**This is not a universal result.** A shape with large radii, compared against a
fixed-count implementation tuned lower, would tessellate faster and coarser than
imgui-painter. What this measures is one look — the one the visual gate had
already validated as *looking correct*, which is the constraint that makes the
comparison meaningful at all.

The point is not that imgui-painter is fast. It is that the 5× reduction in
source lines does not cost performance.

## Allocation behavior

Separately from throughput, the Rust side of a frame allocates **nothing** in the
steady state. This is enforced by `bindings/rust/tests/zero_alloc.rs`, which
installs a process-wide counting global allocator and asserts a zero
steady-state per-frame count.

Real-application numbers from a host under load are in
[Case study: punks2](../case-study-punks.md#performance).
