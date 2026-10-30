package main

import alicorn "alicorn:runtime"
import ui "alicorn:ui"

// These bands depend on presentation inputs, never on the current recording,
// transport state, duration, or status message. Results take the space left over.
Audition_Layout :: struct {
	compact: bool,
	text_scale: f32,
	header, waveform, waveform_controls, footer, transport, volume, gap, height: f32,
}

audition_layout :: proc(app: ^App, ctx: ^ui.UI) -> Audition_Layout {
	observation := alicorn.style_root_accessibility_observation_get(ctx.runtime)
	scale := f32(1)
	if observation.text_scale_known { scale = max(1, observation.text_scale) }
	metrics := alicorn.style_theme_control_metrics(ctx.runtime, app.theme)
	layout := Audition_Layout{
		compact=app.viewport_width < 900,
		text_scale=scale,
		header=max(28, 25.5*scale),
		waveform=WAVEFORM_HEIGHT,
		waveform_controls=max(32, max(metrics.button.minimum_hit_height, 20*scale+2*metrics.button.content_padding_y)),
		footer=max(22, 18*scale+4),
		transport=max(36, max(metrics.button.preferred_height,
			max(metrics.button.minimum_hit_height, 24*scale+2*metrics.button.content_padding_y))),
		volume=max(44, max(metrics.slider.preferred_height,
			max(metrics.slider.minimum_hit_height, 20*scale+2*metrics.slider.content_padding_y))),
		gap=8,
	}
	if layout.compact {
		layout.waveform = 72
		layout.gap = 6
	}
	layout.height = layout.header+layout.waveform+layout.footer+layout.transport+layout.volume+24+4*layout.gap
	return layout
}

waveform_ready :: proc(app: ^App) -> bool {
	return len(app.waveform.peaks) > 0 && app.waveform.duration > 0 &&
		(app.state.playback.status != "loading" || app.state.playback.file == app.waveform.file)
}

audition_status :: proc(app: ^App) -> (label, detail: string) {
	if len(app.state.audio_error) > 0 { return "Error", app.state.audio_error }
	if len(app.state.error) > 0 { return "Error", app.state.error }
	if len(app.error) > 0 { return "Error", app.error }
	if app.waveform_view.pending && waveform_ready(app) { return "Seeking", "Waiting for the requested source position" }
	if app.state.playback.status == "loading" { return "Loading", "Preparing audio" }
	if app.state.playback.status == "playing" { return "Playing", "" }
	if app.state.playback.status == "paused" { return "Paused", "" }
	return "", ""
}
