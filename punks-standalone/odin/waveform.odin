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
	fraction, sent_fraction: f32,
	request_id: u64,
	last_send: time.Time,
	geometry_updates: u64,
	head_fraction, head_width, head_height: f32,
	head_focused, head_pending: bool,
	head_accent: alicorn.Color,
	ready, cached_ready, head_ready: bool,
}

waveform_fraction :: proc(app: ^App) -> f32 {
	if app.waveform_view.pending { return app.waveform_view.fraction }
	if app.waveform.duration <= 0 { return 0 }
	return clamp(f32((app.state.playback.position-app.waveform.start)/app.waveform.duration), 0, 1)
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
	duplicate := v.pending && v.sent && v.sent_fraction == fraction
	v.fraction = fraction
	v.pending = true
	// Latest-wins dragging: send at most 25 seeks/s, then always send release.
	if !final && v.sent && v.dragging && time.since(v.last_send) < 40*time.Millisecond { return }
	if !final && duplicate { return }
	app.next_diagnostic_request_id += 1
	v.request_id = app.next_diagnostic_request_id
	send(app, Command{op="seek", waveform=app.state.waveform, path=app.waveform.file,
		seconds=app.waveform.start+f64(fraction)*app.waveform.duration, request_id=v.request_id})
	v.sent = true
	v.sent_fraction = fraction
	v.last_send = time.now()
}

format_audio_time :: proc(seconds: f64) -> string {
	centiseconds := u64(max(0, seconds)*100+0.5)
	return fmt.tprintf("%d:%02d.%02d", centiseconds/6000, (centiseconds/100)%60, centiseconds%100)
}

render_waveform :: proc(app: ^App, ctx: ^ui.UI, layout: Audition_Layout) {
	v := &app.waveform_view
	v.peaks_node, v.head_node = 0, 0
	v.ready = waveform_ready(app)
	v.accent = alicorn.style_color(ctx, .Accent)
	v.background = alicorn.style_color(ctx, .Editor_Background)
	columns := [1]alicorn.Grid_Track{alicorn.grid_fraction(1)}
	rows := [1]alicorn.Grid_Track{alicorn.grid_fixed(layout.waveform)}
	if alicorn.grid_begin(ctx, alicorn.key_string("waveform-timeline"), columns[:], rows[:],
		style=ui.layout_style(height=layout.waveform, clip=true)) != 0 {
		owner := alicorn.container_begin(ctx, .Container, key="waveform-seek-owner", style=ui.layout_style(height=layout.waveform))
		fraction := waveform_fraction(app) if v.ready else 0
		result := alicorn.slider_f32(ctx, "Waveform position", fraction, 0, 1,
			key=alicorn.key_string("audition-seek"), style=ui.layout_style(height=layout.waveform), disabled=!v.ready)
		_ = alicorn.semantic_range_value_text(ctx, fmt.tprintf("%.2f of %.2f seconds",
			app.waveform.start+f64(fraction)*app.waveform.duration, app.state.playback.duration))
		_ = alicorn.tooltip(ctx, "Click or drag to seek · arrows adjust · Home/End · Space plays or pauses")
		if result.activated { waveform_seek(app, result.value) }
		alicorn.container_end(ctx)
		_ = alicorn.grid_cell(ctx, owner, 0, 0)
		dpi := app.dpi_scale if app.dpi_scale > 0 else 1
		v.peaks_node = alicorn.gpu_geometry_surface(ctx, "waveform-peaks", ui.layout_style(height=layout.waveform), dpi)
		_ = alicorn.grid_cell(ctx, v.peaks_node, 0, 0)
		v.head_node = alicorn.gpu_geometry_surface(ctx, "waveform-playhead", ui.layout_style(height=layout.waveform), dpi)
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
	if waveform_ready(app) {
		position = app.waveform.start+f64(waveform_fraction(app))*app.waveform.duration
		duration = app.state.playback.duration
	}
	numeric := fmt.tprintf("%s / %s", format_audio_time(position), format_audio_time(duration))
	column := 0
	if !layout.compact {
		start := ui.text(ctx, format_audio_time(app.waveform.start) if waveform_ready(app) else "0:00.00",
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
		end := ui.text(ctx, format_audio_time(app.waveform.start+app.waveform.duration) if waveform_ready(app) else "0:00.00",
			key="waveform-end-time", size=12, font=.Monospace,
			style=ui.layout_style(height=layout.footer), text_style=alicorn.Text_Style{overflow=.Ellipsis},
			color=alicorn.style_color(ctx, .Muted_Text))
		_ = alicorn.grid_cell(ctx, end, 0, column, align_x=.End)
	}
	alicorn.grid_end(ctx)
}

waveform_envelope :: proc(peaks: [][2]i16, column, count: int) -> (lo, hi: f32) {
	first := column*len(peaks)/count
	last := min(len(peaks), max(first+1, (column+1)*len(peaks)/count))
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
	if surface.payload_revision == 0 || v.resource != app.state.waveform || v.width != w || v.height != h ||
		v.cached_accent != v.accent || v.cached_background != v.background || v.cached_ready != v.ready {
		segments: [WAVEFORM_BARS+2]alicorn.GPU_Surface_Line_Segment
		// Leave the stock slider focus outline visible at the outer edge.
		segments[0] = {start={2, h/2}, end={w-2, h/2}, thickness=h-4, color=v.background}
		baseline := v.accent
		baseline.a = 0.2
		segments[1] = {start={8, h/2}, end={w-8, h/2}, thickness=1, color=baseline}
		count := min(len(app.waveform.peaks), min(WAVEFORM_BARS, max(1, int((w-16)/1.5))))
		if !v.ready { count = 0 }
		for column in 0..<count {
			lo, hi := waveform_envelope(app.waveform.peaks, column, count)
			x := 8+(f32(column)+0.5)*(w-16)/f32(count)
			top, bottom := h/2-hi*(h/2-8), h/2-lo*(h/2-8)
			segments[column+2] = {start={x, top}, end={x, max(top+1, bottom)}, thickness=1.25, color=v.accent}
		}
		if alicorn.gpu_surface_update_geometry(rt, v.peaks_node, segments[:count+2], nil) {
			v.resource = app.state.waveform
			v.width, v.height = w, h
			v.cached_accent, v.cached_background = v.accent, v.background
			v.cached_ready = v.ready
			v.geometry_updates += 1
		}
	}
	if !v.ready {
		_ = alicorn.gpu_surface_clear(rt, v.head_node)
		v.head_ready = false
		return
	}
	fraction := waveform_fraction(app)
	focused := alicorn.focused_node(rt) == v.seek_node
	head, head_ok := alicorn.gpu_surface_context(rt, v.head_node)
	if !head_ok { return }
	if v.head_ready && head.payload_revision != 0 && v.head_fraction == fraction && v.head_width == w && v.head_height == h &&
		v.head_focused == focused && v.head_pending == v.pending && v.head_accent == v.accent { return }
	x := 8+fraction*(w-16)
	color := v.accent
	color.a = 0.12
	segments: [6]alicorn.GPU_Surface_Line_Segment
	count := 0
	if x > 8 {
		segments[count] = {start={8, h/2}, end={x, h/2}, thickness=h-8, color=color}
		count += 1
	}
	color = ui.rgb(0xFFD293)
	segments[count] = {start={x, 4}, end={x, h-4}, thickness=2, color=color}
	count += 1
	if focused {
		segments[count] = {start={1, 1}, end={w-1, 1}, thickness=1, color=v.accent}; count += 1
		segments[count] = {start={1, h-1}, end={w-1, h-1}, thickness=1, color=v.accent}; count += 1
		segments[count] = {start={1, 1}, end={1, h-1}, thickness=1, color=v.accent}; count += 1
		segments[count] = {start={w-1, 1}, end={w-1, h-1}, thickness=1, color=v.accent}; count += 1
	}
	marker := [1]alicorn.GPU_Surface_Filled_Circle{{center={x, 5}, radius=3, color=color}}
	if alicorn.gpu_surface_update_geometry(rt, v.head_node, segments[:count], marker[:]) {
		v.head_ready = true
		v.head_fraction, v.head_width, v.head_height = fraction, w, h
		v.head_focused, v.head_pending, v.head_accent = focused, v.pending, v.accent
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
