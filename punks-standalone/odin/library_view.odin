package main

import "core:fmt"
import alicorn "alicorn:runtime"
import ui "alicorn:ui"
import host "alicorn:native/sdl_gpu"
import tree_view "alikit:tree_view"

TAB_BAR_HEIGHT :: 36
WAVEFORM_HEIGHT :: 100
WAVEFORM_BARS :: 512

format_bytes :: proc(size: u64) -> string {
	if size >= 1024*1024*1024 {
		return fmt.tprintf("%.1f GB", f64(size)/(1024*1024*1024))
	}
	if size >= 1024*1024 {
		return fmt.tprintf("%.1f MB", f64(size)/(1024*1024))
	}
	if size >= 1024 {
		return fmt.tprintf("%.1f KB", f64(size)/1024)
	}
	return fmt.tprintf("%d B", size)
}

path_basename :: proc(value: string) -> string {
	start := 0
	for index := 0; index < len(value); index += 1 {
		if value[index] == '/' || value[index] == '\\' {
			start = index+1
		}
	}
	return value[start:]
}

punks_entry_button :: proc(
	ctx: ^ui.UI,
	item: Entry,
	selected: bool,
	columns: bool,
	row_height: f32,
) -> bool {
	style := ui.layout_style(.Row, width=-1, height=row_height, padding=6, gap=10, align=.Center, clip=true)
	id, activated := alicorn.button_begin(
		ctx,
		"",
		key=alicorn.key_string(item.id),
		style=style,
		state=alicorn.Button_State{selected=selected},
		content_style=alicorn.button_content_style(.Start, .Center, padding_x=8, padding_y=0),
		variant=.Quiet,
	)
	if id == 0 {
		return false
	}
	states: alicorn.Semantic_States
	if selected { states = alicorn.semantic_states_add(states, .Selected) }
	_ = alicorn.semantic_description(ctx, .Button, item.name,
		states=states, actions=alicorn.semantic_actions_add({}, .Press))
	_ = alicorn.tooltip(ctx, item.name)
	icon_id := alicorn.Icon_ID.File
	if item.directory {
		icon_id = .Folder_Closed
	}
	_ = ui.icon(ctx, icon_id, size=16)
	_ = ui.text(ctx, item.name, key="entry-name", size=14,
		style=ui.layout_style(width=-1, height=20, grow=1),
		text_style=alicorn.Text_Style{overflow=.Ellipsis})
	if columns && !item.directory {
		_ = ui.text(
			ctx,
			item.format,
			style=ui.layout_style(width=76),
			color=alicorn.style_color(ctx, .Muted_Text),
	)
		_ = ui.text(
			ctx,
			format_bytes(item.size),
			style=ui.layout_style(width=88),
			color=alicorn.style_color(ctx, .Muted_Text),
		)
	} else if columns {
		_ = ui.text(ctx, "Folder", style=ui.layout_style(width=76), color=alicorn.style_color(ctx, .Muted_Text))
		_ = ui.text(ctx, "", style=ui.layout_style(width=88))
	}
	alicorn.button_end(ctx)
	return activated
}

punks_folder_item_id :: proc(value: string) -> tree_view.Item_ID {
	hash: u64 = 14_695_981_039_346_656_037
	for character in value { hash = (hash ~ u64(character)) * 1_099_511_628_211 }
	if hash == 0 { hash = 1 }
	return tree_view.Item_ID(hash)
}

punks_folder_tree_window :: proc(app: ^App) -> tree_view.Window {
	if app == nil { return {} }
	window := tree_view.Window{
		total_count=app.state.folder_count,
		offset=0,
		revision=app.state.listing_revision,
	}
	if app.folders.listing_revision != app.state.listing_revision { return window }
	window.offset = app.state.folder_offset
	items := make([]tree_view.Item, len(app.folders.entries), allocator=context.temp_allocator)
	for folder, index in app.folders.entries {
		items[index] = tree_view.Item{
			id=punks_folder_item_id(folder.id),
			label=folder.name,
		}
	}
	window.items = items
	return window
}

punks_folder_tree_icon :: proc(_data: rawptr, _item: tree_view.Item) -> alicorn.Icon_ID {
	return .Folder_Closed
}

punks_folder_tree_row_hook :: proc(_data: rawptr, ctx: ^ui.UI, item: tree_view.Item, _semantic: alicorn.Semantic_ID) {
	_ = alicorn.tooltip(ctx, item.label)
}

punks_folder_tree_apply_event :: proc(app: ^App, event: tree_view.Event) {
	if app == nil { return }
	if event.kind == .Range_Requested {
		offset := max(0, event.index-4)
		if app.requested_folder_revision != app.state.listing_revision || app.requested_folder_offset != offset {
			send(app, Command{op="folder_window", offset=offset, listing_revision=app.state.listing_revision})
			app.requested_folder_revision = app.state.listing_revision
			app.requested_folder_offset = offset
		}
		return
	}
	if event.kind != .Pressed && event.kind != .Activated { return }
	if app.folders.listing_revision != app.state.listing_revision { return }
	for folder in app.folders.entries {
		if punks_folder_item_id(folder.id) == event.id {
			send(app, Command{
				op="enter_folder",
				id=folder.id,
				index=folder.index,
				listing_revision=app.state.listing_revision,
			})
			return
		}
	}
}

render_folder_tree :: proc(app: ^App, ctx: ^ui.UI) {
	if app.folder_tree_revision != app.state.listing_revision {
		app.folder_tree = {}
		app.folder_tree_revision = app.state.listing_revision
	}
	window := punks_folder_tree_window(app)
	event := tree_view.build(ctx, &app.folder_tree, window,
		alicorn.key_string("library-folder-tree"),
		tree_view.Options{
			accessible_label="Folders in this location",
			empty_label="No folders here",
			loading_label="Loading folders…",
			row_height=FOLDER_ROW_HEIGHT,
			row_padding=6,
			row_gap=10,
			hide_disclosure=true,
			icon_proc=punks_folder_tree_icon,
			row_hook_proc=punks_folder_tree_row_hook,
		},
	)
	app.folder_node = app.folder_tree.scroll_node
	punks_folder_tree_apply_event(app, event)
}

render_tab_strip :: proc(app: ^App, rt: ^alicorn.Runtime, ctx: ^ui.UI) {
	items := make([]alicorn.Tab_Bar_Item, len(app.state.tabs), allocator=context.temp_allocator)
	for tab, index in app.state.tabs {
		items[index] = alicorn.Tab_Bar_Item{
			key=alicorn.key_u64(tab.id),
			label=tab.title,
			tooltip=tab.title,
			selected=tab.id == app.state.active_tab,
			closable=len(app.state.tabs) > 1,
			semantic_id=alicorn.Semantic_ID{namespace=PUNKS_TAB_SEMANTIC_NAMESPACE, value=tab.id},
		}
	}
	options := alicorn.DEFAULT_TAB_BAR_OPTIONS
	options.min_tab_width = 136
	options.max_tab_width = 180
	options.height = TAB_BAR_HEIGHT
	options.close_policy = .Selected_Or_Hover
	options.drag_preview_mode = .Source_Subtree
	options.drag_tracking = alicorn.DRAG_TRACKING_HORIZONTAL_MAGNETIC
	options.presentation = .Compact
	options.gap = 0
	options.appearance = .Connected
	options.animate_drag_reorder = true
	options.selected_fill = alicorn.style_color(ctx, .Subtle_Surface)
	options.selected_text = alicorn.style_color(ctx, .Text)
	options.selected_fill_set = true
	options.drag_type = PUNKS_TAB_DRAG_TYPE
	result := alicorn.tab_bar(
		ctx,
		alicorn.key_string("punks-library-tabs"),
		items[:],
		options,
		ui.layout_style(.Row, height=TAB_BAR_HEIGHT, grow=1, clip=true),
	)
	punks_tab_reorder_motion(app, ctx, rt, result.item_nodes)
	if result.action == .Select && result.item_index >= 0 && result.item_index < len(app.state.tabs) {
		send(app, Command{op="tab_switch", tab_id=app.state.tabs[result.item_index].id})
	} else if result.action == .Close && result.item_index >= 0 && result.item_index < len(app.state.tabs) {
		send(app, Command{op="tab_close", tab_id=app.state.tabs[result.item_index].id})
	}
	if ui.button(ctx, "+", key="punks-new-tab", width=32, height=32, variant=.Toolbar) {
		send(app, Command{op="tab_new"})
	}
	_ = alicorn.semantic_description(ctx, .Button, "New tab", actions=alicorn.semantic_actions_add({}, .Press))
	_ = alicorn.tooltip(ctx, "New tab")
}

punks_tab_reorder_motion :: proc(
	app: ^App,
	ctx: ^ui.UI,
	rt: ^alicorn.Runtime,
	tab_nodes: []alicorn.Node_ID,
) {
	if app == nil || ctx == nil || rt == nil || len(tab_nodes) != len(app.state.tabs) { return }
	if app.tab_motion_reset_pending {
		intent := ui.Motion_Intent.Settle
		if app.tab_motion_reset_snap { intent = .Snap }
		for node in tab_nodes {
			_ = ui.motion(ctx, node, ui.Motion_Target{
				translation={},
				clip_policy=ui.Motion_Clip_Policy.Follow_Visual,
				has_translation=true,
			}, intent)
		}
		app.tab_reorder_motion_signature = 0
		app.tab_motion_reset_pending = false
		app.tab_motion_reset_snap = false
		return
	}
	if app.tab_drag_source_id == 0 || len(tab_nodes) < 2 { return }

	order := make([]u64, len(app.state.tabs), allocator=context.temp_allocator)
	for tab, index in app.state.tabs { order[index] = tab.id }
	from, target := -1, -1
	for id, index in order {
		if id == app.tab_drag_source_id { from = index }
		if id == app.tab_drag_target_id { target = index }
	}
	if from >= 0 && target >= 0 &&
		(app.tab_drag_position == .Before || app.tab_drag_position == .After) {
		to := target
		if app.tab_drag_position == .After { to += 1 }
		if from < to { to -= 1 }
		to = min(to, len(order)-1)
		if to != from {
			moving := order[from]
			if from < to {
				for index := from; index < to; index += 1 { order[index] = order[index+1] }
			} else {
				for index := from; index > to; index -= 1 { order[index] = order[index-1] }
			}
			order[to] = moving
		}
	}

	first, first_ok := alicorn.node_info(rt, tab_nodes[0])
	second, second_ok := alicorn.node_info(rt, tab_nodes[1])
	stride: f32 = 0
	if first_ok && second_ok { stride = second.bounds.x-first.bounds.x }
	if stride <= 0 { return }

	signature: u64 = 14_695_981_039_346_656_037
	for id in order { signature = (signature ~ id) * 1_099_511_628_211 }
	signature = (signature ~ u64(transmute(u32)stride)) * 1_099_511_628_211
	if signature == app.tab_reorder_motion_signature { return }

	for node, index in tab_nodes {
		if app.state.tabs[index].id == app.tab_drag_source_id { continue }
		destination := index
		for id, proposed_index in order {
			if id == app.state.tabs[index].id { destination = proposed_index; break }
		}
		delta := f32(destination-index)*stride
		_ = ui.motion(ctx, node, ui.Motion_Target{
			translation=ui.Motion_Vector2{x=f64(delta)},
			clip_policy=ui.Motion_Clip_Policy.Follow_Visual,
			has_translation=true,
		}, .Settle)
	}
	app.tab_reorder_motion_signature = signature
}

render_title_row :: proc(app: ^App, rt: ^alicorn.Runtime, ctx: ^ui.UI) {
	if ui.row(ctx, key="application-tabs", height=42, gap=12, align=.Center, label="application-tabs") {
		_ = ui.text(ctx, "punks.", size=24, style=ui.layout_style(width=92),
			text_style=alicorn.Text_Style{font_weight=600})
		render_tab_strip(app, rt, ctx)
	}
}

render_breadcrumbs :: proc(app: ^App, ctx: ^ui.UI) {
	count := len(app.state.breadcrumbs)
	if count == 0 {
		_ = ui.text(ctx, "Choose a folder to start browsing", color=alicorn.style_color(ctx, .Muted_Text))
		return
	}
	if count > 4 {
		if ui.button(
			ctx,
			app.state.breadcrumbs[0],
			key=alicorn.key_string("breadcrumb-root"),
			width=96,
			height=32,
			variant=.Quiet,
			content_style=alicorn.button_content_style(.Start, .Center, padding_x=4, padding_y=0),
		) {
			send(app, Command{op="breadcrumb", index=0})
		}
		_ = ui.text(ctx, "›", color=alicorn.style_color(ctx, .Muted_Text))
		if ui.button(
			ctx,
			"…",
			key=alicorn.key_string("breadcrumb-overflow"),
			width=32,
			height=32,
			variant=.Quiet,
		) {
			send(app, Command{op="breadcrumb", index=count-4})
		}
		_ = ui.text(ctx, "›", color=alicorn.style_color(ctx, .Muted_Text))
		for index := count-3; index < count; index += 1 {
			if index > count-3 {
				_ = ui.text(ctx, "›", color=alicorn.style_color(ctx, .Muted_Text))
			}
			if ui.button(
				ctx,
				app.state.breadcrumbs[index],
				key=alicorn.key_u64(u64(index)+0xB000),
				width=128,
				height=32,
				variant=.Quiet,
				content_style=alicorn.button_content_style(.Start, .Center, padding_x=4, padding_y=0),
			) {
				send(app, Command{op="breadcrumb", index=index})
			}
		}
		return
	}
	for name, index in app.state.breadcrumbs {
		if index > 0 {
			_ = ui.text(ctx, "›", color=alicorn.style_color(ctx, .Muted_Text))
		}
		if ui.button(
			ctx,
			name,
			key=alicorn.key_u64(u64(index)+0xB000),
			width=128,
			height=32,
			variant=.Quiet,
			content_style=alicorn.button_content_style(.Start, .Center, padding_x=4, padding_y=0),
		) {
			send(app, Command{op="breadcrumb", index=index})
		}
	}
}

render_location_toolbar :: proc(app: ^App, ctx: ^ui.UI) {
	if ui.row(ctx, height=36, gap=8, align=.Center, label="location-toolbar") {
		if ui.button(
			ctx,
			"Up",
			key=alicorn.key_string("navigate-up"),
			width=48,
			height=32,
			state=ui.Button_State{disabled=!app.state.can_navigate_up},
			variant=.Quiet,
		) {
			send(app, Command{op="up"})
		}
		if ui.row(ctx, grow=1, height=32, gap=4, align=.Center, clip=true, label="location-breadcrumbs") {
			render_breadcrumbs(app, ctx)
		}
		if ui.button(ctx, "Open folder", key=alicorn.key_string("open-folder"), width=104, height=32, variant=.Toolbar) {
			open_folder(app)
		}
	}
	if ui.row(ctx, height=38, gap=10, align=.Center, label="search-toolbar") {
		field := ui.text_field(
			ctx,
			"Search",
			&app.search,
			key=alicorn.key_string("search"),
			style=ui.layout_style(.Row, grow=1, height=36),
		)
		app.search_node = field.node
		if field.changed {
			app.pending_search = true
			_ = host.application_schedule_after(app.services.scheduler, .Frequent, 150_000_000)
		}
		if field.submitted {
			app.pending_search = false
			_ = host.application_cancel_scheduled_wake(app.services.scheduler, .Frequent)
			send(app, Command{op="search", query=app.search.text})
		}
		count_label := fmt.tprintf("%d results", app.state.total)
		_ = ui.text(ctx, count_label, color=alicorn.style_color(ctx, .Muted_Text))
	}
}

render_sidebar :: proc(app: ^App, ctx: ^ui.UI) {
	if ui.panel(
		ctx,
		key=alicorn.key_string("library-sidebar-surface"),
		height=-1,
		grow=1,
		padding=12,
		gap=10,
		clip=true,
		radius=5,
		label="Library",
		material=app.materials.recessed,
		physical_height=-1.75,
	) {
		_ = ui.text(ctx, "LIBRARY", size=12, color=alicorn.style_color(ctx, .Muted_Text))
		location := "No folder selected"
		if len(app.state.breadcrumbs) > 0 {
			location = app.state.breadcrumbs[len(app.state.breadcrumbs)-1]
		}
		_ = ui.text(ctx, location, size=17)
		_ = ui.text(ctx, "FOLDERS", size=12, color=alicorn.style_color(ctx, .Muted_Text))
		if len(app.state.breadcrumbs) == 0 {
			app.folder_node = 0
			_ = ui.text(ctx, "Open a sample folder to browse its subfolders.", color=alicorn.style_color(ctx, .Muted_Text))
		} else {
			render_folder_tree(app, ctx)
		}
	}
}

render_results :: proc(app: ^App, ctx: ^ui.UI) {
	columns := app.viewport_width >= 900
	if ui.panel(
		ctx,
		key=alicorn.key_string("results-surface"),
		height=-1,
		grow=1,
		padding=10,
		gap=4,
		clip=true,
		fill=alicorn.style_color(ctx, .Editor_Background),
		radius=5,
		stroke=ui.border(1, alicorn.style_color(ctx, .Border)),
		label="Audio files",
		material=app.materials.recessed,
		physical_height=-1.75,
	) {
		if ui.row(ctx, height=30, padding=6, gap=10, align=.Center, label="results-column-headings") {
			_ = ui.icon(ctx, .File, size=16)
			_ = ui.text(ctx, "Name", style=ui.layout_style(grow=1), color=alicorn.style_color(ctx, .Muted_Text))
			if columns {
				_ = ui.text(ctx, "Format", style=ui.layout_style(width=76), color=alicorn.style_color(ctx, .Muted_Text))
				_ = ui.text(ctx, "Size", style=ui.layout_style(width=88), color=alicorn.style_color(ctx, .Muted_Text))
			}
		}
		if app.state.total == 0 && !app.state.searching {
			message := "Open a folder to browse audio"
			if len(app.state.breadcrumbs) > 0 {
				message = "No matching audio files"
			}
			_ = ui.text(ctx, message, style=ui.layout_style(grow=1), color=alicorn.style_color(ctx, .Muted_Text))
		} else if list, opened := ui.virtual_list(
			ctx,
			app.state.total,
			ROW_HEIGHT,
			key=alicorn.key_string("audio-results"),
			grow=1,
			label="Audio results",
			focusable=true,
		); opened {
			app.list_node = list.scroll.id
			first, last := list.first, list.last
			for position := first; position < last; position += 1 {
				local := position-app.window.offset
				if local >= 0 && local < len(app.window.entries) &&
					app.window.listing_revision == app.state.listing_revision {
					row := app.window.entries[local]
					if punks_entry_button(ctx, row, i64(position) == app.state.selected_index, columns, ROW_HEIGHT) {
						op := "select"
						if row.directory {
							op = "enter"
						}
						send(app, Command{
							op=op,
							id=row.id,
							index=row.index,
							listing_revision=app.state.listing_revision,
							audition=!row.directory,
						})
					}
				} else {
					_ = ui.text(ctx, "Loading…", style=ui.layout_style(height=ROW_HEIGHT))
				}
			}
			outside_window := first < app.window.offset || last > app.window.offset+len(app.window.entries)
			if app.state.ready && last > first && outside_window {
				offset := max(0, first-8)
				if app.requested_revision != app.state.listing_revision || app.requested_offset != offset {
					send(app, Command{
						op="window",
						offset=offset,
						listing_revision=app.state.listing_revision,
					})
					app.requested_revision = app.state.listing_revision
					app.requested_offset = offset
				}
			}
		}
	}
}

render_audio_deck :: proc(app: ^App, ctx: ^ui.UI) {
	layout := audition_layout(app, ctx)
	if ui.panel(
		ctx,
		key=alicorn.key_string("audition-deck"),
		sizing=ui.sizing(height=ui.logical(layout.height)),
		padding=12,
		gap=layout.gap,
		clip=true,
		radius=5,
		stroke=ui.border(1, alicorn.style_color(ctx, .Border)),
		label="Audition",
		material=app.materials.mounted,
		physical_height=1.5,
	) {
		if ui.row(ctx, key="audition-track-info", height=layout.header, gap=10, align=.Center, clip=true, label="audition-track-info") {
			name := app.state.selected.name
			source := app.state.selected.path
			if len(app.state.waveform_file) > 0 && waveform_ready(app) { source = app.state.waveform_file; name = path_basename(source) }
			if app.state.playback.status == "loading" && len(app.state.playback.file) > 0 {
				source = app.state.playback.file
				name = path_basename(source)
			}
			if len(name) == 0 { name = "Select an audio file" }
			status, _ := audition_status(app)
			indicator_color := alicorn.style_color(ctx, .Muted_Text)
			indicator_opacity := f64(0.38)
			switch status {
			case "Playing":
				indicator_color = alicorn.style_color(ctx, .Accent)
				indicator_opacity = 1
			case "Paused":
				indicator_color = alicorn.style_color(ctx, .Accent_Hover)
				indicator_opacity = 0.72
			case "Loading", "Seeking":
				indicator_color = alicorn.style_color(ctx, .Accent_Hover)
				indicator_opacity = 0.82
			case "Error":
				indicator_color = alicorn.style_color(ctx, .Danger)
				indicator_opacity = 1
			}
			indicator := ui.box(ctx, key=alicorn.key_string("audition-status-indicator"),
				size=10, fill=indicator_color, radius=5)
			intent := ui.Motion_Intent.Settle
			if !app.status_indicator_initialized {
				intent = .Snap
				app.status_indicator_initialized = true
			}
			_ = ui.motion(ctx, indicator,
				ui.Motion_Target{opacity=indicator_opacity, has_opacity=true}, intent)
			_ = ui.text(ctx, name, key="audition-track-name", size=17,
				style=ui.layout_style(height=layout.header, grow=1), text_style=alicorn.Text_Style{overflow=.Ellipsis})
			_ = alicorn.tooltip(ctx, source if len(source) > 0 else name)
			metadata := ""
			if source == app.state.selected.path && len(app.state.selected.format) > 0 {
				metadata = fmt.tprintf("%s %s", app.state.selected.format, format_bytes(app.state.selected.size))
			}
			_ = ui.text(ctx, metadata, key="audition-track-metadata",
				style=ui.layout_style(width=120*layout.text_scale, height=layout.header),
				text_style=alicorn.Text_Style{overflow=.Ellipsis}, color=alicorn.style_color(ctx, .Muted_Text))
			if len(metadata) > 0 { _ = alicorn.tooltip(ctx, metadata) }
		}
		render_waveform(app, ctx, layout)
		if ui.row(ctx, key="audition-transport", height=layout.transport, gap=8, align=.Center, clip=true, label="audition-transport") {
			if ui.button(
				ctx,
				"Previous",
				key=alicorn.key_string("audition-previous"),
				width=78,
				height=layout.transport,
				state=ui.Button_State{disabled=len(app.state.selected.id) == 0 || app.state.selected.directory},
				variant=.Quiet,
			) {
				send(app, Command{op="step", delta=-1, audition=true})
			}
			playing := app.state.playback.status == "playing"
			paused := app.state.playback.status == "paused"
			loading := app.state.playback.status == "loading"
			play_label := "Start"
			play_op := "play"
			play_disabled := len(app.state.selected.id) == 0 || app.state.selected.directory
			if playing {
				play_label = "Pause"
				play_op = "pause"
				play_disabled = false
			} else if paused {
				play_op = "resume"
				play_disabled = false
			} else if loading {
				play_disabled = true
			}
			if ui.button(
				ctx,
				play_label,
				key=alicorn.key_string("audition-start-pause"),
				width=88,
				height=layout.transport,
				state=ui.Button_State{disabled=play_disabled},
				variant=.Primary,
			) {
				send(app, Command{op=play_op})
			}
			if ui.button(
				ctx,
				"Stop",
				key=alicorn.key_string("audition-stop"),
				width=60,
				height=layout.transport,
				state=ui.Button_State{disabled=app.state.playback.status == "idle"},
				variant=.Quiet,
			) {
				send(app, Command{op="stop"})
			}
			if ui.button(
				ctx,
				"Next",
				key=alicorn.key_string("audition-next"),
				width=66,
				height=layout.transport,
				state=ui.Button_State{disabled=len(app.state.selected.id) == 0 || app.state.selected.directory},
				variant=.Quiet,
			) {
				send(app, Command{op="step", delta=1, audition=true})
			}

		}
		if ui.row(ctx, key="audition-volume-row", height=layout.volume, gap=8, align=.Center, clip=true, label="audition-volume-row") {
			volume := app.state.volume
			if ui.slider_f32(
				ctx,
				"Volume",
				&volume,
				0,
				1,
				step=0.01,
				key=alicorn.key_string("audition-volume"),
				height=layout.volume,
				grow=1,
			) {
				send(app, Command{op="volume", value=volume})
			}
			_ = ui.text(
				ctx,
				fmt.tprintf("%.0f%%", f64(volume*100)),
				key="audition-volume-value",
				style=ui.layout_style(width=42*layout.text_scale, height=layout.volume),
				text_style=alicorn.Text_Style{overflow=.Ellipsis},
				color=alicorn.style_color(ctx, .Muted_Text),
			)
		}
	}
}

render_workspace :: proc(app: ^App, rt: ^alicorn.Runtime, ctx: ^ui.UI) {
	// A scoped UI helper must enclose its children in its positive if block.
	// Filling panes use height=-1; the helper default is content-sized.
	if ui.panel(ctx, key="punks-workspace", height=-1, grow=1,
		padding=10, gap=8, clip=true, fill=alicorn.style_color(ctx, .Window_Background)) {
		render_title_row(app, rt, ctx)
		split := alicorn.split_begin(
			ctx, alicorn.key_string("punks-library-split"), .Horizontal, 216,
			min_first=170, min_second=360,
			style=ui.layout_style(.Row, grow=1, gap=8, clip=true),
			label="Punks library workspace",
		)
		alicorn.split_first_begin(ctx, split)
		render_sidebar(app, ctx)
		alicorn.split_first_end(ctx, split)
		alicorn.split_divider(ctx, split, thickness=2, hit_size=10)
		alicorn.split_second_begin(ctx, split)
		if ui.column(ctx, height=-1, grow=1, gap=8, clip=true, label="audio-browser-main") {
			render_location_toolbar(app, ctx)
			render_results(app, ctx)
			render_audio_deck(app, ctx)
		}
		alicorn.split_second_end(ctx, split)
		alicorn.split_end(ctx, split)
		message := fmt.tprintf("%d items", app.state.total)
		if !app.state.ready { message = "Starting backend…" }
		if app.state.searching { message = "Searching audio…" }
		if app.state.scanning && app.state.scan_total > 0 {
			message = fmt.tprintf("Indexing %d / %d", app.state.scan_done, app.state.scan_total)
		}
		if len(app.state.audio_error) > 0 { message = app.state.audio_error }
		if len(app.state.error) > 0 { message = app.state.error }
		if len(app.error) > 0 { message = app.error }
		_ = ui.text(ctx, message, size=12, key="application-status", style=ui.layout_style(height=18),
			text_style=alicorn.Text_Style{overflow=.Ellipsis}, color=alicorn.style_color(ctx, .Muted_Text))
	}
}
