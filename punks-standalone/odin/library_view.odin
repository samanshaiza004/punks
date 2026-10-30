package main

import "core:fmt"
import alicorn "alicorn:runtime"
import ui "alicorn:ui"
import host "alicorn:native/sdl_gpu"

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
	options.min_tab_width = 96
	options.max_tab_width = 210
	options.height = TAB_BAR_HEIGHT
	options.presentation = .Compact
	options.gap = 0
	options.appearance = .Connected
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
	if result.action == .Select && result.item_index >= 0 && result.item_index < len(app.state.tabs) {
		send(app, Command{op="tab_switch", tab_id=app.state.tabs[result.item_index].id})
	} else if result.action == .Close && result.item_index >= 0 && result.item_index < len(app.state.tabs) {
		send(app, Command{op="tab_close", tab_id=app.state.tabs[result.item_index].id})
	}
	if event, found := alicorn.drag_event_take(rt); found &&
		event.kind == .Dropped &&
		event.drag_type == PUNKS_TAB_DRAG_TYPE &&
		event.source.namespace == PUNKS_TAB_SEMANTIC_NAMESPACE &&
		event.target.namespace == PUNKS_TAB_SEMANTIC_NAMESPACE {
		send(app, Command{
			op="tab_reorder",
			tab_id=event.source.value,
			tab_target_id=event.target.value,
			after=event.position == .After,
		})
	}
	if ui.button(ctx, "+", key="punks-new-tab", width=32, height=32, variant=.Toolbar) {
		send(app, Command{op="tab_new"})
	}
	_ = alicorn.semantic_description(ctx, .Button, "New tab", actions=alicorn.semantic_actions_add({}, .Press))
	_ = alicorn.tooltip(ctx, "New tab")
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
			_ = ui.text(ctx, "Open a sample folder to browse its subfolders.", color=alicorn.style_color(ctx, .Muted_Text))
		} else if list, opened := ui.virtual_list(
			ctx,
			app.state.folder_count,
			FOLDER_ROW_HEIGHT,
			key=alicorn.key_string("library-folder-list"),
			grow=1,
			label="Folders in this location",
			focusable=true,
		); opened {
			app.folder_node = list.scroll.id
			first, last := list.first, list.last
			for position := first; position < last; position += 1 {
				local := position-app.state.folder_offset
				if app.folders.listing_revision == app.state.listing_revision &&
					local >= 0 && local < len(app.folders.entries) {
					folder := app.folders.entries[local]
					if punks_entry_button(ctx, folder, false, false, FOLDER_ROW_HEIGHT) {
						send(app, Command{
							op="enter_folder",
							id=folder.id,
							index=folder.index,
							listing_revision=app.state.listing_revision,
						})
					}
				} else {
					_ = ui.text(ctx, "Loading folders…", style=ui.layout_style(height=FOLDER_ROW_HEIGHT))
				}
			}
			outside_window := first < app.state.folder_offset ||
				last > app.state.folder_offset+len(app.folders.entries)
			if last > first && outside_window {
				offset := max(0, first-4)
				if app.requested_folder_revision != app.state.listing_revision ||
					app.requested_folder_offset != offset {
					send(app, Command{
						op="folder_window",
						offset=offset,
						listing_revision=app.state.listing_revision,
					})
					app.requested_folder_revision = app.state.listing_revision
					app.requested_folder_offset = offset
				}
			}
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
	if ui.panel(
		ctx,
		key=alicorn.key_string("audition-deck"),
		sizing=ui.sizing(height=ui.fit_content()),
		padding=12,
		gap=8,
		clip=true,
		radius=5,
		stroke=ui.border(1, alicorn.style_color(ctx, .Border)),
		label="Audition",
		material=app.materials.mounted,
		physical_height=1.5,
	) {
		if ui.row(ctx, gap=10, align=.Center, label="audition-track-info") {
			name := app.state.selected.name
			if len(app.state.waveform_file) > 0 {
				name = path_basename(app.state.waveform_file)
			}
			if len(name) == 0 {
				name = "Select an audio file"
			}
			_ = ui.text(ctx, name, size=17, style=ui.layout_style(grow=1))
			selected_is_loaded := app.state.waveform_file == "" || app.state.selected.path == app.state.waveform_file
			if selected_is_loaded && len(app.state.selected.format) > 0 {
				_ = ui.text(ctx, app.state.selected.format, color=alicorn.style_color(ctx, .Muted_Text))
			}
			if selected_is_loaded && app.state.selected.size > 0 {
				_ = ui.text(ctx, format_bytes(app.state.selected.size), color=alicorn.style_color(ctx, .Muted_Text))
			}
		}
		render_waveform(app, ctx)
		if ui.row(ctx, gap=8, align=.Center, label="audition-transport") {
			if ui.button(
				ctx,
				"Previous",
				key=alicorn.key_string("audition-previous"),
				width=78,
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
				play_label = "Loading…"
				play_disabled = true
			}
			if ui.button(
				ctx,
				play_label,
				key=alicorn.key_string("audition-start-pause"),
				width=88,
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
				state=ui.Button_State{disabled=len(app.state.selected.id) == 0 || app.state.selected.directory},
				variant=.Quiet,
			) {
				send(app, Command{op="step", delta=1, audition=true})
			}
			_ = ui.text(
				ctx,
				fmt.tprintf("%s · %.1f / %.1f s", app.state.playback.status, app.state.playback.position, app.state.playback.duration),
				style=ui.layout_style(grow=1),
				color=alicorn.style_color(ctx, .Muted_Text),
			)
		}
		if ui.row(ctx, gap=8, align=.Center, label="audition-volume-row") {
			volume := app.state.volume
			if ui.slider_f32(
				ctx,
				"Volume",
				&volume,
				0,
				1,
				step=0.01,
				key=alicorn.key_string("audition-volume"),
				grow=1,
			) {
				send(app, Command{op="volume", value=volume})
			}
			_ = ui.text(
				ctx,
				fmt.tprintf("%.0f%%", f64(volume*100)),
				style=ui.layout_style(width=42),
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
		_ = ui.text(ctx, message, size=12, style=ui.layout_style(height=18), color=alicorn.style_color(ctx, .Muted_Text))
	}
}
