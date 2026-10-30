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
	app.waveform.file = app.state.selected.path
	app.waveform.duration = 1.25
	app.waveform.peaks = make([][2]i16, WAVEFORM_BARS, allocator=context.temp_allocator)
	for &peak, index in app.waveform.peaks {
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
		"a loaded waveform should add its seek interaction in that same description")
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
	app.waveform.file = app.state.selected.path
	app.waveform.duration = 1.25
	app.waveform.peaks = make([][2]i16, WAVEFORM_BARS, allocator=context.temp_allocator)
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
		"loaded playback should expose waveform seek semantics and the volume slider")
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

	testing.expect(t, alicorn.focus(&rt, seek_id), "the waveform should accept keyboard focus")
	app.state.playback.position = 0.8
	alicorn.invalidate_root(&rt, "playhead position updated")
	_ = build(&app, &rt, 620, 720, 1)
	updated_seek_id, updated_seek_found := alicorn.node_by_key(&rt, alicorn.key_string("audition-seek"), .Slider)
	testing.expect(t, updated_seek_found && updated_seek_id == seek_id &&
		alicorn.focused_node(&rt) == seek_id,
		"playhead updates should retain waveform identity and focus")
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

@(test)
test_waveform_retains_signed_geometry_and_updates_only_the_playhead :: proc(t: ^testing.T) {
	app := layout_test_app()
	app.search, _ = ui.text_buffer("", context.allocator)
	defer ui.text_buffer_destroy(&app.search)
	app.waveform = Waveform_Window{duration=89.3, peaks=make([][2]i16, 512, allocator=context.temp_allocator)}
	for &peak in app.waveform.peaks { peak = {8192, 24576} }
	app.state.playback = Playback{status="playing", position=12, duration=89.3}
	rt := alicorn.new_runtime({0, 0, 1200, 800})
	defer alicorn.destroy_runtime(&rt)
	_ = build(&app, &rt, 1200, 800, 1.5)
	v := &app.waveform_view
	peak_info, peak_ok := alicorn.node_info(&rt, v.peaks_node)
	head_info, head_ok := alicorn.node_info(&rt, v.head_node)
	seek_info, seek_ok := alicorn.node_info(&rt, v.seek_node)
	testing.expect(t, peak_ok && head_ok && seek_ok && peak_info.bounds == head_info.bounds &&
		peak_info.bounds == seek_info.bounds, "the painted waveform, playhead and seek hit region must coincide")
	if !peak_ok || !head_ok { return }
	segments := rt.nodes[v.peaks_node].surface_segments
	testing.expect(t, len(segments) > 64 && len(segments) <= 514,
		"detail should adapt to width, using the bounded 512-peak envelope instead of 64 widgets")
	testing.expect(t, segments[2].start.y < peak_info.bounds.h/2 && segments[2].end.y < peak_info.bounds.h/2,
		"a positive-only source must stay above the center, preserving its signed envelope")
	peaks, _ := alicorn.gpu_surface_context(&rt, v.peaks_node)
	head, _ := alicorn.gpu_surface_context(&rt, v.head_node)
	builds := app.builds
	invalidated_before := rt.invalidated
	app.state.playback.position = 42
	waveform_update(&app, &rt)
	updated_peaks, _ := alicorn.gpu_surface_context(&rt, v.peaks_node)
	updated_head, _ := alicorn.gpu_surface_context(&rt, v.head_node)
	testing.expect(t, peaks.payload_revision == updated_peaks.payload_revision &&
		head.payload_revision < updated_head.payload_revision && app.builds == builds && rt.invalidated == invalidated_before,
		fmt.tprintf("playhead-only update: peaks %d/%d, head %d/%d, builds %d/%d, invalidation %v/%v",
		peaks.payload_revision, updated_peaks.payload_revision, head.payload_revision, updated_head.payload_revision,
		builds, app.builds, invalidated_before, rt.invalidated))
	semantic, ok := alicorn.semantic_node_lookup(&rt, alicorn.semantic_visual_id(v.seek_node))
	testing.expect(t, ok && semantic.role == .Slider && semantic.has_numeric_value &&
		alicorn.semantic_actions_has(semantic.actions, .Set_Value), "waveform seeking must expose a real accessible range control")
}

@(test)
test_waveform_drag_uses_source_axis_coalesces_and_commits_final_target :: proc(t: ^testing.T) {
	app := layout_test_app()
	app.search, _ = ui.text_buffer("", context.allocator)
	defer ui.text_buffer_destroy(&app.search)
	app.waveform = Waveform_Window{start=20, duration=5, peaks=make([][2]i16, 512, allocator=context.temp_allocator)}
	app.state.playback = Playback{status="paused", position=21, duration=120}
	rt := alicorn.new_runtime({0, 0, 960, 720})
	defer alicorn.destroy_runtime(&rt)
	_ = build(&app, &rt, 960, 720, 1)
	bounds := layout_test_bounds(t, &rt, "audition-seek", .Slider)
	event := alicorn.Pointer_Event{kind=.Down, x=bounds.x+8+(bounds.w-16)*0.25,
		y=bounds.y+bounds.h/2, button=alicorn.POINTER_BUTTON_PRIMARY}
	target := alicorn.process_pointer(&rt, event)
	testing.expect(t, target == app.waveform_view.seek_node, "inert graphics must pass clicks to their seek interaction owner")
	waveform_pointer(&app, &rt, event, target)
	testing.expect(t, app.waveform_view.pending && abs(app.waveform_view.fraction-0.25) < 0.001 &&
		abs(app.waveform.start+f64(app.waveform_view.fraction)*app.waveform.duration-21.25) < 0.001,
		"seeking must use the visible source interval, including a nonzero window origin")
	for index in 0..<100 {
		event.kind = .Move
		event.x = bounds.x+f32(index)*bounds.w/100
		target = alicorn.process_pointer(&rt, event)
		waveform_pointer(&app, &rt, event, target)
	}
	before_release := app.next_diagnostic_request_id
	event.kind = .Up
	event.x = bounds.x+bounds.w+200
	target = alicorn.process_pointer(&rt, event)
	waveform_pointer(&app, &rt, event, target)
	testing.expect(t, app.next_diagnostic_request_id == before_release+1 && before_release < 100 &&
		app.waveform_view.fraction == 1 && !app.waveform_view.dragging,
		"scrubbing must coalesce moves, retain capture outside the waveform, and always submit the clamped release target")
	testing.expect(t, app.state.playback.status == "paused", "a seek must not optimistically change transport state")
	testing.expect(t, alicorn.focus(&rt, app.waveform_view.seek_node), "waveform should be keyboard focusable")
	_ = alicorn.set_focused_slider_bound(&rt, .Minimum)
	_ = build(&app, &rt, 960, 720, 1)
	testing.expect(t, app.waveform_view.fraction == 0, "Home should seek to the beginning of the displayed source interval")
}
