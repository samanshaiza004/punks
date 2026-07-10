// Compile-only proof that include/imgui_painter.h works against *any*
// type with PrimReserve/PrimWriteVtx/PrimWriteIdx/_VtxCurrentIdx methods,
// not just a real ImDrawList -- driven by
// tests/fluent_header_compiles.rs, which never links or runs this file,
// only compiles it. A real ImDrawList's method signatures
// (PrimReserve(int,int); PrimWriteVtx(const ImVec2&, const ImVec2&,
// ImU32); PrimWriteIdx(ImDrawIdx)) are mirrored here structurally, not by
// including <imgui.h> -- that's the whole point of the mock.
#include "imgui_painter.h"

#include <cstdint>
#include <vector>

namespace {

struct MockVec2 {
    float x, y;
};

struct MockDrawList {
    unsigned int _VtxCurrentIdx = 0;
    std::vector<MockVec2> positions;
    std::vector<std::uint32_t> colors;
    std::vector<std::uint16_t> indices;

    void PrimReserve(int idx_count, int vtx_count) {
        indices.reserve(indices.size() + static_cast<std::size_t>(idx_count));
        positions.reserve(positions.size() + static_cast<std::size_t>(vtx_count));
        colors.reserve(colors.size() + static_cast<std::size_t>(vtx_count));
    }

    void PrimWriteVtx(const MockVec2 &pos, const MockVec2 &uv, std::uint32_t col) {
        (void)uv;
        positions.push_back(pos);
        colors.push_back(col);
        ++_VtxCurrentIdx;
    }

    void PrimWriteIdx(std::uint16_t idx) { indices.push_back(idx); }
};

} // namespace

void fluent_header_mock_entry_point() {
    MockDrawList dl;
    ip_rect rect{{0.0f, 0.0f}, {40.0f, 40.0f}};

    // Solid fill + border, the ip_color overload of fill().
    ip::Painter(ip_vec2{0.5f, 0.5f}, rect, 4.0f)
        .fill(ip_color(0xFFAABBCCu))
        .border(ip_border{1.0f, ip_color(0xFF000000u)})
        .draw(dl);

    // Shadow + gradient fill, the ip_gradient overload of fill() -- proves
    // both overloads resolve without ambiguity.
    const ip_color_stop stops[2] = {
        {0.0f, ip_color(0xFFFFFFFFu)},
        {1.0f, ip_color(0xFF000000u)},
    };
    const ip_gradient gradient{
        IP_GRADIENT_LINEAR, {0.0f, 0.0f}, {0.0f, 40.0f}, stops, 2,
    };
    ip::Painter(ip_vec2{0.5f, 0.5f}, rect, 4.0f)
        .shadow(ip_shadow{{0.0f, 2.0f}, 8.0f, 0.0f, ip_color(0x3C000000u), false})
        .fill(gradient)
        .draw(dl);
}
