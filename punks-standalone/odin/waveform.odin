package main

import "core:fmt"
import "core:time"
import alicorn "alicorn:runtime"
import ui "alicorn:ui"

// The ordinary slider owns focus, keyboard/AX range actions and drag capture.
// Two inert geometry surfaces cover its stock presentation. Peaks are uploaded
// only for a resource/size/theme change; the independent playhead is tiny.
Waveform_View :: struct {
	peaks_node, head_node, seek_node: alicorn.Node_ID,
	resource: Resource,
	width, height: f32,
	accent, background: alicorn.Color,
	cached_accent, cached_background: alicorn.Color,
	pending, dragging, sent: bool,
	fraction: f32,
	zoom, center_fraction: f32,
	pending_time, sent_time: f64,
	request_id: u64,
	last_send: time.Time,
	geometry_updates: u64,
	head_fraction, head_width, head_height: f32,
	cached_view_start, cached_view_span: f32,
	head_focused, head_pending: bool,
	head_visible, cached_head_visible: bool,
	head_accent: alicorn.Color,
	ready, cached_ready, head_ready: bool,
}

waveform_zoom_level :: proc(v: ^Waveform_View) -> f32 {
	if v.zoom <= 0 { return 1 }
	return clamp(v.zoom, 1, 128)
}

waveform_view_fraction :: proc(app: ^App) -> (start, span: f32) {
	zoom := waveform_zoom_level(&app.waveform_view)
	span = 1/zoom
	center := app.waveform_view.center_fraction
	if center <= 0 { center = 0.5 }
	start = clamp(center-span/2, 0, 1-span)
	return start, span
}

waveform_visible_interval :: proc(app: ^App) -> (start, duration: f64) {
	normalized_start, normalized_span := waveform_view_fraction(app)
	if app.waveform.duration <= 0 { return app.waveform.start, 0 }
	return app.waveform.start+f64(normalized_start)*app.waveform.duration,
		f64(normalized_span)*app.waveform.duration
}

waveform_position :: proc(app: ^App) -> f64 {
	if app.waveform_view.pending { return app.waveform_view.pending_time }
	return app.state.playback.position
}

waveform_fraction :: proc(app: ^App) -> f32 {
	view_start, view_duration := waveform_visible_interval(app)
	if view_duration <= 0 { return 0 }
	return clamp(f32((waveform_position(app)-view_start)/view_duration), 0, 1)
}

waveform_zoom_to :: proc(app: ^App, requested_zoom: f32, anchor_fraction := f32(0.5)) {
	v := &app.waveform_view
	old_start, old_span := waveform_view_fraction(app)
	anchor_position := clamp(anchor_fraction, 0, 1)
	anchor := old_start+anchor_position*old_span
	zoom := clamp(requested_zoom, 1, 128)
	new_span := 1/zoom
	new_start := clamp(anchor-anchor_position*new_span, 0, 1-new_span)
	v.zoom = zoom
	v.center_fraction = new_start+new_span/2
}

waveform_pan_by :: proc(app: ^App, view_widths: f32) {
	v := &app.waveform_view
	start, span := waveform_view_fraction(app)
	new_start := clamp(start+view_widths*span, 0, 1-span)
	v.center_fraction = new_start+span/2
}

toggle_playback :: proc(app: ^App) {
	op := "play"
	if app.state.playback.status == "playing" { op = "pause" }
	if app.state.playback.status == "paused" { op = "resume" }
	send(app, Command{op=op})
}

waveform_seek :: proc(app: ^App, requested: f32, final := false) {
	if !waveform_ready(app) { return }
	v := &app.waveform_view
	fraction := clamp(requested, 0, 1)
	view_start, view_duration := waveform_visible_interval(app)
	if view_duration <= 0 { return }
	target := view_start+f64(fraction)*view_duration
	duplicate := v.pending && v.sent && v.sent_time == target
	v.fraction = fraction
	v.pending_time = target
	v.pending = true
	// Latest-wins dragging: send at most 25 seeks/s, then always send release.
	if !final && v.sent && v.dragging && time.since(v.last_send) < 40*time.Millisecond { return }
	if !final && duplicate { return }
	app.next_diagnostic_request_id += 1
	v.request_id = app.next_diagnostic_request_id
	send(app, Command{op="seek", waveform=app.state.waveform, path=app.waveform.file,
		seconds=target, request_id=v.request_id})
	v.sent = true
	v.sent_time = target
	v.last_send = time.now()
}

format_audio_time :: proc(seconds: f64) -> string {
	centiseconds := u64(max(0, seconds)*100+0.5)
	return fmt.tprintf("%d:%02d.%02d", centiseconds/6000, (centiseconds/100)%60, centiseconds%100)
}

waveform_zoom_button_semantics :: proc(ctx: ^ui.UI, label: string, disabled: bool) {
	states: alicorn.Semantic_States
	actions: alicorn.Semantic_Actions
	if disabled {
		states = alicorn.semantic_states_add(states, .Disabled)
	} else {
		actions = alicorn.semantic_actions_add(actions, .Press)
	}
	_ = alicorn.semantic_description(ctx, .Button, label, states=states, actions=actions)
}

render_waveform :: proc(app: ^App, ctx: ^ui.UI, layout: Audition_Layout) {
	v := &app.waveform_view
	v.peaks_node, v.head_node = 0, 0
	v.ready = waveform_ready(app)
	v.accent = alicorn.style_color(ctx, .Accent)
	v.background = alicorn.style_color(ctx, .Editor_Background)
	if ui.column(ctx, key="waveform-layout", height=layout.waveform, gap=2, clip=true) {
		zoom := waveform_zoom_level(v)
		if ui.row(ctx, key="waveform-zoom-controls", height=layout.waveform_controls,
			gap=4, align=.Center, justify=.End, clip=true) {
			zoom_out_disabled := !v.ready || zoom <= 1.001
			if ui.button(ctx, "-", key=alicorn.key_string("waveform-zoom-out"), width=32,
				height=layout.waveform_controls, state=ui.Button_State{disabled=zoom_out_disabled}, variant=.Quiet) {
				waveform_zoom_to(app, zoom/1.25)
			}
			waveform_zoom_button_semantics(ctx, "Zoom out", zoom_out_disabled)
			_ = alicorn.tooltip(ctx, "Zoom out")
			fit_disabled := !v.ready || zoom <= 1.001
			if ui.button(ctx, "Fit", key=alicorn.key_string("waveform-zoom-fit"), width=40,
				height=layout.waveform_controls, state=ui.Button_State{disabled=fit_disabled}, variant=.Quiet) {
				waveform_zoom_to(app, 1)
			}
			waveform_zoom_button_semantics(ctx, "Fit waveform", fit_disabled)
			_ = alicorn.tooltip(ctx, "Fit the entire recording")
			zoom_in_disabled := !v.ready || zoom >= 127.999
			if ui.button(ctx, "+", key=alicorn.key_string("waveform-zoom-in"), width=32,
				height=layout.waveform_controls, state=ui.Button_State{disabled=zoom_in_disabled}, variant=.Quiet) {
				waveform_zoom_to(app, zoom*1.25)
			}
			waveform_zoom_button_semantics(ctx, "Zoom in", zoom_in_disabled)
			_ = alicorn.tooltip(ctx, "Zoom in")
			zoom_label := ui.text(ctx, fmt.tprintf("%.1fx", f64(zoom)), key="waveform-zoom-level",
				style=ui.layout_style(width=40, height=layout.waveform_controls), font=.Monospace,
				text_style=alicorn.Text_Style{overflow=.Ellipsis}, color=alicorn.style_color(ctx, .Muted_Text))
			_ = zoom_label
		}
		graph_height := max(1, layout.waveform-layout.waveform_controls-2)
	columns := [1]alicorn.Grid_Track{alicorn.grid_fraction(1)}
	rows := [1]alicorn.Grid_Track{alicorn.grid_fixed(graph_height)}
	if alicorn.grid_begin(ctx, alicorn.key_string("waveform-timeline"), columns[:], rows[:],
		style=ui.layout_style(height=graph_height, clip=true)) != 0 {
		owner := alicorn.container_begin(ctx, .Container, key="waveform-seek-owner", style=ui.layout_style(height=graph_height))
		fraction := waveform_fraction(app) if v.ready else 0
		result := alicorn.slider_f32(ctx, "Waveform position", fraction, 0, 1,
			key=alicorn.key_string("audition-seek"), style=ui.layout_style(height=graph_height), disabled=!v.ready)
		view_start, view_duration := waveform_visible_interval(app)
		_ = alicorn.semantic_range_value_text(ctx, fmt.tprintf("%.2f of %.2f seconds",
			view_start+f64(fraction)*view_duration, app.state.playback.duration))
		_ = alicorn.tooltip(ctx, "Click or drag to seek; Ctrl+wheel zooms at the pointer; wheel pans; Fit shows the whole recording; arrows, Home and End seek; Space plays or pauses")
		if result.activated { waveform_seek(app, result.value) }
		alicorn.container_end(ctx)
		_ = alicorn.grid_cell(ctx, owner, 0, 0)
		dpi := app.dpi_scale if app.dpi_scale > 0 else 1
		v.peaks_node = alicorn.gpu_geometry_surface(ctx, "waveform-peaks", ui.layout_style(height=graph_height), dpi)
		_ = alicorn.grid_cell(ctx, v.peaks_node, 0, 0)
		v.head_node = alicorn.gpu_geometry_surface(ctx, "waveform-playhead", ui.layout_style(height=graph_height), dpi)
		_ = alicorn.grid_cell(ctx, v.head_node, 0, 0)
		placeholder := ""
		if !v.ready {
			placeholder = "Select an audio file to see its waveform"
			if app.state.playback.status == "loading" { placeholder = "Loading waveform…" }
			if len(app.state.audio_error) > 0 { placeholder = "Could not load audio" }
		}
		placeholder_id := ui.text(ctx, placeholder, key="waveform-placeholder", size=14,
			style=ui.layout_style(height=24*layout.text_scale), text_style=alicorn.Text_Style{overflow=.Ellipsis},
			color=alicorn.style_color(ctx, .Muted_Text))
		_ = alicorn.grid_cell(ctx, placeholder_id, 0, 0, align_x=.Center, align_y=.Center)
		alicorn.grid_end(ctx)
	}
	}
	render_time_footer(app, ctx, layout)
}

render_time_footer :: proc(app: ^App, ctx: ^ui.UI, layout: Audition_Layout) {
	columns: [4]alicorn.Grid_Track
	count := 2
	columns[0], columns[1] = alicorn.grid_fraction(1), alicorn.grid_fixed(72*layout.text_scale)
	if !layout.compact {
		count = 4
		columns = {alicorn.grid_fraction(1), alicorn.grid_fraction(3),
			alicorn.grid_fixed(72*layout.text_scale), alicorn.grid_fraction(1)}
	}
	rows := [1]alicorn.Grid_Track{alicorn.grid_fixed(layout.footer)}
	if alicorn.grid_begin(ctx, alicorn.key_string("waveform-footer"), columns[:count], rows[:], gap_x=8,
		style=ui.layout_style(height=layout.footer, clip=true)) == 0 { return }
	position, duration := f64(0), f64(0)
	view_start, view_duration := waveform_visible_interval(app)
	if waveform_ready(app) {
		position = waveform_position(app)
		duration = app.state.playback.duration
	}
	numeric := fmt.tprintf("%s / %s", format_audio_time(position), format_audio_time(duration))
	column := 0
	if !layout.compact {
		start := ui.text(ctx, format_audio_time(view_start) if waveform_ready(app) else "0:00.00",
			key="waveform-start-time", size=12, font=.Monospace,
			style=ui.layout_style(height=layout.footer), text_style=alicorn.Text_Style{overflow=.Ellipsis},
			color=alicorn.style_color(ctx, .Muted_Text))
		_ = alicorn.grid_cell(ctx, start, 0, column)
		column += 1
	}
	current := ui.text(ctx, numeric, key="waveform-current-time", size=12, font=.Monospace,
		style=ui.layout_style(height=layout.footer), text_style=alicorn.Text_Style{overflow=.Ellipsis},
		color=alicorn.style_color(ctx, .Muted_Text))
	_ = alicorn.tooltip(ctx, numeric)
	_ = alicorn.grid_cell(ctx, current, 0, column)
	column += 1
	status, detail := audition_status(app)
	status_id := ui.text(ctx, status, key="waveform-status", size=12,
		style=ui.layout_style(height=layout.footer), text_style=alicorn.Text_Style{overflow=.Ellipsis},
		color=alicorn.style_color(ctx, .Muted_Text))
	if len(detail) > 0 { _ = alicorn.tooltip(ctx, detail) }
	_ = alicorn.grid_cell(ctx, status_id, 0, column)
	column += 1
	if !layout.compact {
		end := ui.text(ctx, format_audio_time(view_start+view_duration) if waveform_ready(app) else "0:00.00",
			key="waveform-end-time", size=12, font=.Monospace,
			style=ui.layout_style(height=layout.footer), text_style=alicorn.Text_Style{overflow=.Ellipsis},
			color=alicorn.style_color(ctx, .Muted_Text))
		_ = alicorn.grid_cell(ctx, end, 0, column, align_x=.End)
	}
	alicorn.grid_end(ctx)
}

waveform_envelope :: proc(peaks: [][2]i16, range_start, range_end, column, count: int) -> (lo, hi: f32) {
	range_count := range_end-range_start
	first := range_start+column*range_count/count
	last := min(range_end, max(first+1, range_start+(column+1)*range_count/count))
	low, high: i16 = 32767, -32768
	for peak in peaks[first:last] {
		low = min(low, min(peak[0], peak[1]))
		high = max(high, max(peak[0], peak[1]))
	}
	return clamp(f32(low)/32767, -1, 1), clamp(f32(high)/32767, -1, 1)
}

waveform_update :: proc(app: ^App, rt: ^alicorn.Runtime) {
	v := &app.waveform_view
	v.seek_node, _ = alicorn.node_by_key(rt, alicorn.key_string("audition-seek"), .Slider)
	surface, ok := alicorn.gpu_surface_context(rt, v.peaks_node)
	if !ok || surface.logical_bounds.w <= 16 || surface.logical_bounds.h <= 0 { return }
	w, h := surface.logical_bounds.w, surface.logical_bounds.h
	view_start, view_span := waveform_view_fraction(app)
	if surface.payload_revision == 0 || v.resource != app.state.waveform || v.width != w || v.height != h ||
		v.cached_accent != v.accent || v.cached_background != v.background || v.cached_ready != v.ready ||
		v.cached_view_start != view_start || v.cached_view_span != view_span {
		segments: [WAVEFORM_BARS+2]alicorn.GPU_Surface_Line_Segment
		// Leave the stock slider focus outline visible at the outer edge.
		segments[0] = {start={2, h/2}, end={w-2, h/2}, thickness=h-4, color=v.background}
		baseline := v.accent
		baseline.a = 0.2
		segments[1] = {start={8, h/2}, end={w-8, h/2}, thickness=1, color=baseline}
		peak_count := len(app.waveform.peaks)
		peak_start := max(0, min(peak_count-1, int(f32(peak_count)*view_start))) if peak_count > 0 else 0
		peak_end := max(peak_start+1, min(peak_count, int(f32(peak_count)*(view_start+view_span)+0.999))) if peak_count > 0 else 0
		visible_peaks := max(0, peak_end-peak_start)
		count := min(visible_peaks, min(WAVEFORM_BARS, max(1, int((w-16)/1.5))))
		if !v.ready { count = 0 }
		for column in 0..<count {
			lo, hi := waveform_envelope(app.waveform.peaks, peak_start, peak_end, column, count)
			x := 8+(f32(column)+0.5)*(w-16)/f32(count)
			top, bottom := h/2-hi*(h/2-8), h/2-lo*(h/2-8)
			segments[column+2] = {start={x, top}, end={x, max(top+1, bottom)}, thickness=1.25, color=v.accent}
		}
		if alicorn.gpu_surface_update_geometry(rt, v.peaks_node, segments[:count+2], nil) {
			v.resource = app.state.waveform
			v.width, v.height = w, h
			v.cached_accent, v.cached_background = v.accent, v.background
			v.cached_ready = v.ready
			v.cached_view_start, v.cached_view_span = view_start, view_span
			v.geometry_updates += 1
		}
	}
	if !v.ready {
		_ = alicorn.gpu_surface_clear(rt, v.head_node)
		v.head_ready = false
		return
	}
	view_start_time, view_duration := waveform_visible_interval(app)
	position_fraction := f32((waveform_position(app)-view_start_time)/view_duration)
	v.head_visible = position_fraction >= 0 && position_fraction <= 1
	fraction := clamp(position_fraction, 0, 1)
	focused := alicorn.focused_node(rt) == v.seek_node
	head, head_ok := alicorn.gpu_surface_context(rt, v.head_node)
	if !head_ok { return }
	if v.head_ready && head.payload_revision != 0 && v.head_fraction == fraction && v.head_width == w && v.head_height == h &&
		v.head_focused == focused && v.head_pending == v.pending && v.head_visible == v.cached_head_visible &&
		v.head_accent == v.accent { return }
	x := 8+fraction*(w-16)
	color := v.accent
	color.a = 0.12
	segments: [6]alicorn.GPU_Surface_Line_Segment
	count := 0
	if v.head_visible && x > 8 {
		segments[count] = {start={8, h/2}, end={x, h/2}, thickness=h-8, color=color}
		count += 1
	}
	color = ui.rgb(0xFFD293)
	if v.head_visible {
		segments[count] = {start={x, 4}, end={x, h-4}, thickness=2, color=color}
		count += 1
	}
	if focused {
		segments[count] = {start={1, 1}, end={w-1, 1}, thickness=1, color=v.accent}; count += 1
		segments[count] = {start={1, h-1}, end={w-1, h-1}, thickness=1, color=v.accent}; count += 1
		segments[count] = {start={1, 1}, end={1, h-1}, thickness=1, color=v.accent}; count += 1
		segments[count] = {start={w-1, 1}, end={w-1, h-1}, thickness=1, color=v.accent}; count += 1
	}
	marker: [1]alicorn.GPU_Surface_Filled_Circle
	marker_count := 0
	if v.head_visible {
		marker[0] = {center={x, 5}, radius=3, color=color}
		marker_count = 1
	}
	if alicorn.gpu_surface_update_geometry(rt, v.head_node, segments[:count], marker[:marker_count]) {
		v.head_ready = true
		v.head_fraction, v.head_width, v.head_height = fraction, w, h
		v.head_focused, v.head_pending, v.head_accent = focused, v.pending, v.accent
		v.cached_head_visible = v.head_visible
	}
}

waveform_pointer :: proc(data: rawptr, rt: ^alicorn.Runtime, event: alicorn.Pointer_Event, target: alicorn.Node_ID) {
	app := cast(^App)data
	v := &app.waveform_view
	if event.kind == .Cancel {
		v.dragging = false
		v.pending = false
		return
	}
	if event.kind == .Down && target == v.seek_node && v.seek_node != 0 &&
		(event.button == 0 || event.button == alicorn.POINTER_BUTTON_PRIMARY) {
		v.dragging = true
		v.sent = false
	} else if !v.dragging { return }
	if !waveform_ready(app) { v.dragging = false; return }
	info, ok := alicorn.node_info(rt, v.seek_node)
	if !ok { v.dragging = false; return }
	fraction := clamp((event.x-info.bounds.x-8)/max(1, info.bounds.w-16), 0, 1)
	if event.kind == .Up {
		waveform_seek(app, fraction, true)
		v.dragging = false
	} else if event.kind == .Down || event.kind == .Move {
		waveform_seek(app, fraction)
		if event.kind == .Down && event.click_count == 2 { toggle_playback(app) }
	}
	waveform_update(app, rt)
}

waveform_scroll :: proc(data: rawptr, rt: ^alicorn.Runtime, event: alicorn.Scroll_Event) {
	app := cast(^App)data
	v := &app.waveform_view
	if !waveform_ready(app) || v.dragging { return }
	info, ok := alicorn.node_info(rt, v.seek_node)
	if !ok || event.x < info.bounds.x || event.x > info.bounds.x+info.bounds.w ||
		event.y < info.bounds.y || event.y > info.bounds.y+info.bounds.h { return }

	delta_y := event.delta_y
	if abs(delta_y) < 0.001 { delta_y = f32(event.ticks_y) }
	if event.modifiers.control || event.modifiers.super {
		if abs(delta_y) < 0.001 { return }
		factor := 1+min(abs(delta_y)*0.15, 0.5)
		zoom := waveform_zoom_level(v)
		if delta_y < 0 { zoom *= factor } else { zoom /= factor }
		anchor := clamp((event.x-info.bounds.x-8)/max(1, info.bounds.w-16), 0, 1)
		waveform_zoom_to(app, zoom, anchor)
	} else {
		delta := event.delta_x
		if event.modifiers.shift || abs(delta) < 0.001 { delta = delta_y }
		if abs(delta) < 0.001 { return }
		waveform_pan_by(app, delta*0.08)
	}
	alicorn.invalidate_root(rt, "waveform viewport changed")
	waveform_update(app, rt)
}
