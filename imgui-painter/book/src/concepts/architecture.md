# Architecture: core and adapter

```
imgui-painter core     (C++, compiled via cc; ZERO Dear ImGui / cimgui
                        dependency — pure math in, a generic vertex/index
                        mesh out)
        ↑ C API (capi/imgui_painter_c.h)
imgui-painter Rust adapter  (bindings/rust — copies the core's mesh into a
                        real ImDrawList through imgui-sys's own public
                        PrimReserve/PrimWriteVtx/PrimWriteIdx calls)
        ↑
host app (via imgui-rs/imgui-sys)
```

## The core knows nothing about ImGui

`src/painter.cpp` does tessellation, gradients, shadows, and borders. It does not
include an ImGui header, does not link cimgui, and has no ImGui types in its
signatures. Geometry and colors go in; a generic vertex/index mesh comes out.

Two things follow from that:

1. **A host's adapter rides whatever ImGui build the host already linked.** The
   core does not care which version, which backend, or which build flags.
2. **The core is testable and portable on its own.** It is also the reason the
   same C ABI can serve future C, Zig, C#, and Python bindings — see
   [The C ABI](../c-abi.md).

## The adapter writes only through public prim APIs

The adapter copies the core's mesh into a real `ImDrawList` using
`PrimReserve`, `PrimWriteVtx`, and `PrimWriteIdx`. It never writes into
`ImDrawList`'s internal buffers.

This was a deliberate correction, not the original design. An earlier plan had
the core write straight into `ImDrawList`'s vertex and index buffers, so that
even the adapter would need no ImGui dependency.

That was rejected. `ImDrawList`'s public *fields* are stable to read, but its
*invariants* are not something a third party is entitled to maintain:

- write-pointer bookkeeping,
- texture and clip-rect stacking,
- large-mesh vertex-offset handling (`ImDrawCmd::VtxOffset`).

Those are maintained by methods like `PrimReserve`, are not covered by any ABI
guarantee, and **have changed across Dear ImGui versions**. Reimplementing them
would mean silently re-breaking on every upstream release.

The core/adapter split gets the same "core has zero ImGui dependency" property
without taking responsibility for invariants only Dear ImGui itself should own.

## One imgui-sys build

Cargo must resolve the adapter and the host's imgui-rs to a **single**
`imgui-sys`. If it resolves two, you get two Dear ImGui instances with separate
contexts, separate font atlases, and separate draw data — which renders garbage
or crashes.

```sh
cargo tree -d | grep imgui-sys
```

Run that after any dependency change. It is the cheapest check in the project and
it catches the most expensive failure.

## Where version coupling lives

It is worth being precise about this, because it is easy to assume the whole
library is pinned to one ImGui release.

| Layer | Depends on ImGui version? |
|---|---|
| C++ core (`src/painter.cpp`) | No — no ImGui dependency at all |
| Adapter (`adapter.rs`) | No — `PrimReserve`/`PrimWrite*` are stable |
| Style data types (`Material`, `StateColors`, …) | No — plain data |
| Recipe builders (`raised_button`, `panel`, …) | No |
| `decorate_*` | **Yes** — reconstructs widget chrome geometry |
| `recipes::apply_imgui_colors` | **Yes** — names newer color roles |

Only the last two rows are pinned. Everything else works against any reasonable
Dear ImGui. See [The compatibility contract](../decorators/contract.md).
