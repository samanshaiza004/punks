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
	if app.waveform.duration <= 0 || len(app.waveform.peaks) == 0 { return }
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

render_waveform :: proc(app: ^App, ctx: ^ui.UI) {
	v := &app.waveform_view
	v.peaks_node, v.head_node = 0, 0
	if len(app.waveform.peaks) == 0 || app.waveform.duration <= 0 {
		_ = ui.text(ctx, "Select an audio file to see its waveform", style=ui.layout_style(height=40),
			color=alicorn.style_color(ctx, .Muted_Text))
		return
	}
	v.accent = alicorn.style_color(ctx, .Accent)
	v.background = alicorn.style_color(ctx, .Editor_Background)
	columns := [1]alicorn.Grid_Track{alicorn.grid_fraction(1)}
	rows := [1]alicorn.Grid_Track{alicorn.grid_fixed(WAVEFORM_HEIGHT)}
	if alicorn.grid_begin(ctx, alicorn.key_string("waveform-timeline"), columns[:], rows[:],
		style=ui.layout_style(height=WAVEFORM_HEIGHT, clip=true)) != 0 {
		owner := alicorn.container_begin(ctx, .Container, key="waveform-seek-owner", style=ui.layout_style(height=WAVEFORM_HEIGHT))
		fraction := waveform_fraction(app)
		result := alicorn.slider_f32(ctx, "Waveform position", fraction, 0, 1,
			key=alicorn.key_string("audition-seek"), style=ui.layout_style(height=WAVEFORM_HEIGHT))
		_ = alicorn.semantic_range_value_text(ctx, fmt.tprintf("%.2f of %.2f seconds",
			app.waveform.start+f64(fraction)*app.waveform.duration, app.state.playback.duration))
		_ = alicorn.tooltip(ctx, "Click or drag to seek · arrows adjust · Home/End · Space plays or pauses")
		if result.activated { waveform_seek(app, result.value) }
		alicorn.container_end(ctx)
		_ = alicorn.grid_cell(ctx, owner, 0, 0)
		dpi := app.dpi_scale if app.dpi_scale > 0 else 1
		v.peaks_node = alicorn.gpu_geometry_surface(ctx, "waveform-peaks", ui.layout_style(height=WAVEFORM_HEIGHT), dpi)
		_ = alicorn.grid_cell(ctx, v.peaks_node, 0, 0)
		v.head_node = alicorn.gpu_geometry_surface(ctx, "waveform-playhead", ui.layout_style(height=WAVEFORM_HEIGHT), dpi)
		_ = alicorn.grid_cell(ctx, v.head_node, 0, 0)
		alicorn.grid_end(ctx)
	}
	if ui.row(ctx, gap=8, align=.Center) {
		_ = ui.text(ctx, fmt.tprintf("%.2f s", app.waveform.start), size=12,
			style=ui.layout_style(grow=1), color=alicorn.style_color(ctx, .Muted_Text))
		_ = ui.text(ctx, fmt.tprintf("%.2f / %.2f s%s",
			app.waveform.start+f64(waveform_fraction(app))*app.waveform.duration,
			app.state.playback.duration, " · seeking" if v.pending else ""),
			size=12, color=alicorn.style_color(ctx, .Muted_Text))
		_ = ui.text(ctx, fmt.tprintf("%.2f s", app.waveform.start+app.waveform.duration), size=12,
			color=alicorn.style_color(ctx, .Muted_Text))
	}
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
		v.cached_accent != v.accent || v.cached_background != v.background {
		segments: [WAVEFORM_BARS+2]alicorn.GPU_Surface_Line_Segment
		// Leave the stock slider focus outline visible at the outer edge.
		segments[0] = {start={2, h/2}, end={w-2, h/2}, thickness=h-4, color=v.background}
		baseline := v.accent
		baseline.a = 0.2
		segments[1] = {start={8, h/2}, end={w-8, h/2}, thickness=1, color=baseline}
		count := min(len(app.waveform.peaks), min(WAVEFORM_BARS, max(1, int((w-16)/1.5))))
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
			v.geometry_updates += 1
		}
	}
	fraction := waveform_fraction(app)
	focused := alicorn.focused_node(rt) == v.seek_node
	head, head_ok := alicorn.gpu_surface_context(rt, v.head_node)
	if !head_ok { return }
	if head.payload_revision != 0 && v.head_fraction == fraction && v.head_width == w && v.head_height == h &&
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
