package main

import "core:fmt"
import "core:testing"
import alicorn "alicorn:runtime"
import ui "alicorn:ui"

layout_test_app :: proc() -> App {
	rows := make([]Entry, 40, allocator=context.temp_allocator)
	for &row, index in rows {
		row = Entry{
			id=fmt.tprintf("fixture-audio-%d", index), index=index,
			name=fmt.tprintf("808 - Long sample name %02d.wav", index),
			format="wav", size=128000,
		}
	}
	tabs := make([]Tab_View, 2, allocator=context.temp_allocator)
	tabs[0], tabs[1] = Tab_View{id=1, title="Drum Machines"}, Tab_View{id=2, title="Field recordings"}
	crumbs := make([]string, 2, allocator=context.temp_allocator)
	crumbs[0], crumbs[1] = "Samples", "Drum Machines"
	return App{
		state=State{
			ready=true, total=len(rows), count=len(rows), listing_revision=1,
			tabs=tabs, active_tab=1, selected_index=0, selected=rows[0],
			breadcrumbs=crumbs, can_navigate_up=true,
		},
		window=Entry_Window{v=1, listing_revision=1, total=len(rows), entries=rows},
		folders=Folder_Window{v=1, listing_revision=1},
	}
}

layout_test_bounds :: proc(t: ^testing.T, rt: ^alicorn.Runtime, key: string, kind: alicorn.Node_Kind) -> alicorn.Rect {
	id, found := alicorn.node_by_key(rt, alicorn.key_string(key), kind)
	testing.expect(t, found, fmt.tprintf("Expected retained node %s", key))
	info, valid := alicorn.node_info(rt, id)
	testing.expect(t, valid, fmt.tprintf("Expected geometry for %s", key))
	return info.bounds
}

@(test)
test_workspace_geometry_survives_resize :: proc(t: ^testing.T) {
	app := layout_test_app()
	app.search, _ = ui.text_buffer("", context.allocator)
	defer ui.text_buffer_destroy(&app.search)
	rt := alicorn.new_runtime({0, 0, 960, 720})
	defer alicorn.destroy_runtime(&rt)
	for size in ([4][2]int{{960, 720}, {800, 640}, {1200, 800}, {960, 720}}) {
		rt.viewport = {0, 0, f32(size[0]), f32(size[1])}
		alicorn.invalidate_root(&rt, "workspace resize regression")
		_ = build(&app, &rt, size[0], size[1], 1)
		header := layout_test_bounds(t, &rt, "application-tabs", .Container)
		results := layout_test_bounds(t, &rt, "results-surface", .Container)
		deck := layout_test_bounds(t, &rt, "audition-deck", .Container)
		workspace := layout_test_bounds(t, &rt, "punks-workspace", .Container)
		list := layout_test_bounds(t, &rt, "audio-results", .Scroll_Region)
		testing.expect(t, header.y == 10 && header.h == 42,
			"the header must stay at the top, without an empty expanding sibling")
		testing.expect(t, workspace.x == 0 && workspace.y == 0 &&
			workspace.w == f32(size[0]) && workspace.h == f32(size[1]),
			"the themed workbench must cover the complete viewport")
		testing.expect(t, results.h >= 180 && list.h >= 140,
			"results and their scroll viewport must receive remaining vertical space")
		testing.expect(t, results.y > header.y+header.h && deck.y >= results.y+results.h,
			"header, results and deck must remain in their own vertical bands")
		testing.expect(t, deck.h == 186 && deck.y+deck.h <= f32(size[1])-26,
			"the listening deck must remain above the status line")
		testing.expect(t, rt.diagnostic == "", "the layout must have no identity or scope diagnostics")
	}
}

@(test)
test_result_buttons_render_one_filename_and_keep_accessible_names :: proc(t: ^testing.T) {
	app := layout_test_app()
	app.search, _ = ui.text_buffer("", context.allocator)
	defer ui.text_buffer_destroy(&app.search)
	rt := alicorn.new_runtime({0, 0, 960, 720})
	defer alicorn.destroy_runtime(&rt)
	_ = build(&app, &rt, 960, 720, 1)
	filename := app.window.entries[0].name
	text_count, button_label_count := 0, 0
	for id in rt.order {
		node := rt.nodes[id]
		if node.kind == .Text && node.text == filename { text_count += 1 }
		if node.kind == .Button && node.label == filename { button_label_count += 1 }
	}
	testing.expect(t, text_count == 2 && button_label_count == 0,
		"the first filename appears once in its result row and once in the deck; the row button must not paint a duplicate label")
	row, found := alicorn.node_by_key(&rt, alicorn.key_string(app.window.entries[0].id), .Button)
	testing.expect(t, found, "the audio result button must be retained")
	semantic, exported := alicorn.semantic_node_lookup(&rt, alicorn.semantic_visual_id(row))
	testing.expect(t, exported && semantic.label == filename &&
		alicorn.semantic_actions_has(semantic.actions, .Press),
		"custom row content must preserve its accessible name and Press action")
}
