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
		testing.expect(t, deck.h > 0 && deck.y+deck.h <= f32(size[1])-26,
			"the content-sized listening deck must remain above the status line")
		testing.expect(t, rt.diagnostic == "", "the layout must have no identity or scope diagnostics")
	}
}

@(test)
test_audition_deck_and_results_negotiate_in_same_update :: proc(t: ^testing.T) {
	app := layout_test_app()
	app.search, _ = ui.text_buffer("", context.allocator)
	defer ui.text_buffer_destroy(&app.search)
	rt := alicorn.new_runtime({0, 0, 960, 720})
	defer alicorn.destroy_runtime(&rt)
	_ = build(&app, &rt, 960, 720, 1)
	initial_deck := layout_test_bounds(t, &rt, "audition-deck", .Container)
	initial_results := layout_test_bounds(t, &rt, "results-surface", .Container)
	initial_list := layout_test_bounds(t, &rt, "audio-results", .Scroll_Region)
	initial_builds := app.builds

	app.state.waveform_file = app.state.selected.path
	app.state.waveform_peaks = make([][2]i16, WAVEFORM_BARS, allocator=context.temp_allocator)
	for &peak, index in app.state.waveform_peaks {
		peak = [2]i16{i16(5000+index*200), i16(-4000-index*150)}
	}
	app.state.playback = Playback{status="ready", duration=1.25}
	alicorn.invalidate_root(&rt, "preview waveform and seek control became available")
	_ = build(&app, &rt, 960, 720, 1)
	loaded_deck := layout_test_bounds(t, &rt, "audition-deck", .Container)
	loaded_results := layout_test_bounds(t, &rt, "results-surface", .Container)
	loaded_list := layout_test_bounds(t, &rt, "audio-results", .Scroll_Region)
	_, has_seek_control := alicorn.node_by_key(&rt, alicorn.key_string("audition-seek"), .Slider)

	testing.expect(t, app.builds == initial_builds+1,
		"preview adoption should complete through one new application description")
	testing.expect(t, has_seek_control,
		"a loaded duration should add the seek control in that same description")
	testing.expect(t, loaded_deck.h > initial_deck.h,
		"the content-sized deck should grow when the seek control appears")
	testing.expect(t, loaded_results.h < initial_results.h && loaded_list.h < initial_list.h,
		"the results surface and virtualized viewport should give up the deck's added height immediately")
	testing.expect(t, loaded_results.h >= 180 && loaded_list.h >= 140,
		"the expanded deck must leave a usable results area at the supported viewport")
	testing.expect(t, rt.diagnostic == "", "content changes must not produce layout diagnostics")
}

@(test)
test_audition_controls_fit_narrow_large_text_and_retain_focus :: proc(t: ^testing.T) {
	app := layout_test_app()
	app.search, _ = ui.text_buffer("", context.allocator)
	defer ui.text_buffer_destroy(&app.search)
	app.state.waveform_file = app.state.selected.path
	app.state.waveform_peaks = make([][2]i16, WAVEFORM_BARS, allocator=context.temp_allocator)
	app.state.playback = Playback{
		status="playing", file=app.state.selected.path, position=0.4, duration=2,
	}
	app.state.volume = 0.7
	rt := alicorn.new_runtime({0, 0, 620, 720})
	defer alicorn.destroy_runtime(&rt)
	_ = alicorn.style_root_accessibility_observation_set(
		&rt,
		alicorn.Accessibility_Appearance_Observation{text_scale=1.4, text_scale_known=true},
	)
	_ = build(&app, &rt, 620, 720, 1)

	deck := layout_test_bounds(t, &rt, "audition-deck", .Container)
	previous := layout_test_bounds(t, &rt, "audition-previous", .Button)
	start_pause := layout_test_bounds(t, &rt, "audition-start-pause", .Button)
	stop := layout_test_bounds(t, &rt, "audition-stop", .Button)
	next := layout_test_bounds(t, &rt, "audition-next", .Button)
	start_id, start_found := alicorn.node_by_key(&rt, alicorn.key_string("audition-start-pause"), .Button)
	stop_id, stop_found := alicorn.node_by_key(&rt, alicorn.key_string("audition-stop"), .Button)
	seek_id, has_seek := alicorn.node_by_key(&rt, alicorn.key_string("audition-seek"), .Slider)
	volume_id, has_volume := alicorn.node_by_key(&rt, alicorn.key_string("audition-volume"), .Slider)
	button_ordered := previous.x+previous.w <= start_pause.x &&
		start_pause.x+start_pause.w <= stop.x && stop.x+stop.w <= next.x
	controls_inside := previous.x >= deck.x && next.x+next.w <= deck.x+deck.w &&
		previous.y >= deck.y && next.y+next.h <= deck.y+deck.h
	testing.expect(t, start_found && rt.nodes[start_id].label == "Pause",
		"playing state should expose the Pause action")
	testing.expect(t, stop_found && rt.nodes[stop_id].label == "Stop",
		"Stop should remain a separate action")
	testing.expect(t, has_seek && has_volume,
		"loaded playback should expose both seek and volume sliders")
	testing.expect(t, button_ordered && controls_inside,
		"transport buttons should remain ordered and inside the audition deck at narrow width and 140% text scale")
	if !has_seek || !has_volume { return }

	testing.expect(t, alicorn.focus(&rt, volume_id), "the volume slider should accept keyboard focus")
	app.state.playback.status = "paused"
	app.state.volume = 0.55
	alicorn.invalidate_root(&rt, "paused playback and volume state adopted")
	_ = build(&app, &rt, 620, 720, 1)
	updated_start_id, updated_start_found := alicorn.node_by_key(&rt, alicorn.key_string("audition-start-pause"), .Button)
	updated_volume_id, updated_volume_found := alicorn.node_by_key(&rt, alicorn.key_string("audition-volume"), .Slider)
	testing.expect(t, updated_start_found && rt.nodes[updated_start_id].label == "Start",
		"paused playback should offer Start to resume")
	testing.expect(t, updated_volume_found && updated_volume_id == volume_id &&
		alicorn.focused_node(&rt) == volume_id,
		"updating playback state should retain volume-slider identity and focus")

	testing.expect(t, alicorn.focus(&rt, seek_id), "the seek slider should accept keyboard focus")
	app.state.playback.position = 0.8
	alicorn.invalidate_root(&rt, "playhead position updated")
	_ = build(&app, &rt, 620, 720, 1)
	updated_seek_id, updated_seek_found := alicorn.node_by_key(&rt, alicorn.key_string("audition-seek"), .Slider)
	testing.expect(t, updated_seek_found && updated_seek_id == seek_id &&
		alicorn.focused_node(&rt) == seek_id,
		"playhead updates should retain seek-slider identity and focus")
	testing.expect(t, rt.diagnostic == "", "narrow high-text transport layout should have no runtime diagnostic")
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
