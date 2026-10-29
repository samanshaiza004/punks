package main

import "core:encoding/json"
import "core:fmt"
import "core:os"
import "core:thread"
import arena "core:mem/virtual"
import alicorn "alicorn:runtime"
import ui "alicorn:ui"
import host "alicorn:native/sdl_gpu"

ROW_HEIGHT :: 32
FOLDER_ROW_HEIGHT :: 30
PUNKS_TAB_SEMANTIC_NAMESPACE :: u64(0x50554E4B53544142)
PUNKS_TAB_DRAG_TYPE :: alicorn.Drag_Type(0x50554E4B)

App :: struct {
	session: rawptr,
	transport: rawptr,
	api: ^Caliber_API,
	worker: ^thread.Thread,
	waker: host.Application_Waker,
	services: host.Application_Services,
	search: ui.Text_Buffer,
	theme: alicorn.Style_Theme_ID,
	viewport_width: int,
	search_node: alicorn.Node_ID,
	list_node: alicorn.Node_ID,
	folder_node: alicorn.Node_ID,
	state: State,
	window: Entry_Window,
	folders: Folder_Window,
	revision: u64,
	state_memory: [2]arena.Arena,
	entry_memory: [2]arena.Arena,
	state_slot: int,
	entry_slot: int,
	error: string,
	initial_path: string,
	no_audio: bool,
	pending_search: bool,
	keyboard_selection: bool,
	requested_revision: u64,
	requested_offset: int,
	requested_folder_revision: u64,
	requested_folder_offset: int,
	builds: u64,
	smoke: bool,
	smoke_audio: bool,
	smoke_stage: int,
	smoke_complete: bool,
	idle_builds: u64,
	idle_revision: u64,
}

send :: proc(app: ^App, command: Command) {
	if app.session == nil {
		return
	}

	c := command
	c.v = 1
	if c.tab_id == 0 {
		c.tab_id = app.state.active_tab
	}
	bytes, err := json.marshal(c, allocator=context.temp_allocator)
	if err != nil {
		app.error = "Could not encode command"
		return
	}

	status := punks_dispatch(app.session, raw_data(bytes), uintptr(len(bytes)))
	if status != OK {
		app.error = "Backend command was rejected; try again"
	}
}

read_state :: proc(app: ^App) -> bool {
	for _ in 0..<3 {
		publication: Publication
		if app.api.read_state(app.transport, &publication) != OK {
			return false
		}
		if publication.revision == app.revision {
			app.api.release_state(&publication)
			return false
		}
		if publication.schema != 1 || publication.len > 64*1024 {
			app.api.release_state(&publication)
			app.error = "Unsupported backend state"
			return false
		}

		state_slot := 1 - app.state_slot
		arena.arena_free_all(&app.state_memory[state_slot])
		state_allocator := arena.arena_allocator(&app.state_memory[state_slot])
		next_state: State
		err := json.unmarshal(publication.data[:int(publication.len)], &next_state, allocator=state_allocator)
		if err != nil || next_state.v != 1 || !next_state.ready {
			app.api.release_state(&publication)
			app.error = "Backend state could not be decoded"
			return false
		}

		changed_resource := next_state.entries != app.state.entries ||
			next_state.folders != app.state.folders
		next_window := app.window
		next_folders := app.folders
		entry_slot := app.entry_slot
		if changed_resource {
			view: Resource_View
			status := app.api.map_resource(
				app.transport,
			next_state.entries.id,
			next_state.entries.generation,
				&view,
			)
			if status != OK {
				app.api.release_state(&publication)
				continue
			}
			if view.len > 1024*1024 {
				app.api.release_resource_view(&view)
				app.api.release_state(&publication)
				app.error = "Entry resource exceeds protocol limit"
				return false
			}

			entry_slot = 1 - app.entry_slot
			arena.arena_free_all(&app.entry_memory[entry_slot])
			entry_allocator := arena.arena_allocator(&app.entry_memory[entry_slot])
			err = json.unmarshal(view.data[:int(view.len)], &next_window, allocator=entry_allocator)
			app.api.release_resource_view(&view)
			if err != nil ||
				next_window.v != 1 ||
				next_window.listing_revision != next_state.listing_revision ||
				next_window.offset != next_state.offset ||
				len(next_window.entries) > 128 {
				app.api.release_state(&publication)
				app.error = "Entry window failed revision validation"
				return false
			}

			folder_view: Resource_View
			status = app.api.map_resource(
				app.transport,
				next_state.folders.id,
				next_state.folders.generation,
				&folder_view,
			)
			if status != OK {
				app.api.release_state(&publication)
				continue
			}
			if folder_view.len > 1024*1024 {
				app.api.release_resource_view(&folder_view)
				app.api.release_state(&publication)
				app.error = "Folder resource exceeds protocol limit"
				return false
			}
			err = json.unmarshal(
				folder_view.data[:int(folder_view.len)],
				&next_folders,
				allocator=entry_allocator,
			)
			app.api.release_resource_view(&folder_view)
			if err != nil ||
				next_folders.v != 1 ||
				next_folders.listing_revision != next_state.listing_revision ||
				next_folders.offset != next_state.folder_offset ||
				len(next_folders.entries) > 128 {
				app.api.release_state(&publication)
				app.error = "Folder window failed revision validation"
				return false
			}
		}

		if next_state.active_tab != app.state.active_tab ||
			(next_state.query != app.state.query && !app.pending_search) {
			_ = ui.text_buffer_set(&app.search, next_state.query)
			app.pending_search = false
			app.requested_revision = 0
			app.requested_folder_revision = 0
		}
		app.revision = publication.revision
		app.api.release_state(&publication)
		app.state = next_state
		app.window = next_window
		app.folders = next_folders
		app.state_slot = state_slot
		app.entry_slot = entry_slot
		return true
	}
	return false
}

wait_backend :: proc(data: rawptr) {
	app := cast(^App)data
	sequence: u64 = 0
	for {
		status := app.api.wait_wake(app.transport, sequence, &sequence)
		if status == STOPPED {
			return
		}
		if status != OK {
			return
		}
		host.application_wake(app.waker)
	}
}

start :: proc(data: rawptr, waker: host.Application_Waker) {
	app := cast(^App)data
	app.waker = waker
	app.api = caliber_get_api(1)
	if app.api == nil ||
		app.api.abi_version != 1 ||
		app.api.struct_size < u32(size_of(Caliber_API)) {
		app.error = "Incompatible Caliber ABI"
		return
	}

	flags: u32 = 0
	if app.no_audio {
		flags |= 1
	}
	if app.smoke {
		flags |= 2
	}
	status := punks_create(1, flags, &app.session, &app.transport)
	if status != OK {
		app.error = "Could not start Punks backend"
		return
	}

	app.worker = thread.create_and_start_with_data(app, wait_backend, name="punks-caliber-wait")
	if app.worker == nil {
		app.error = "Could not start backend wake waiter"
		return
	}
	if len(app.initial_path) > 0 {
		send(app, Command{op="open", path=app.initial_path})
	}
	if app.smoke {
		_ = host.application_schedule_after(app.services.scheduler, .Opportunistic, 10_000_000_000)
	}
}

stop :: proc(data: rawptr) {
	app := cast(^App)data
	if app.session != nil {
		_ = punks_stop(app.session)
	}
	if app.worker != nil {
		thread.join(app.worker)
		thread.destroy(app.worker)
		app.worker = nil
	}
	if app.session != nil {
		_ = punks_destroy(app.session)
		app.session = nil
		app.transport = nil
	}
	if app.smoke {
		fmt.println(
			"punks_smoke",
			"complete",
			app.smoke_complete,
			"audio",
			app.smoke_audio,
			"builds",
			app.builds,
			"revision",
			app.revision,
		)
		if !app.smoke_complete {
			fmt.eprintln(
				"Punks vertical slice did not complete",
				"stage",
				app.smoke_stage,
				"listing_revision",
				app.state.listing_revision,
				"resource_revision",
				app.window.listing_revision,
				"command_sequence",
				app.state.command_sequence,
				"request_id",
				app.state.last_request_id,
				"playback",
				app.state.playback.status,
				"position",
				app.state.playback.position,
				"selected",
				app.state.selected.name,
				"backend_error",
				app.state.error,
				"audio_error",
				app.state.audio_error,
				"ui_error",
				app.error,
			)
			os.exit(1)
		}
	}
}

services :: proc(data: rawptr, value: host.Application_Services) {
	app := cast(^App)data
	app.services = value
}

open_folder :: proc(app: ^App) {
	if !host.ShowFileDialog(
		app.services.dialogs,
		host.File_Dialog_Request{
			id=1,
			kind=.Open_Folder,
			title="Open samples folder",
			initial_location=app.state.directory,
		},
	) {
		app.error = "Folder dialog is unavailable"
	}
}

dialog :: proc(data: rawptr, rt: ^alicorn.Runtime, result: ^host.File_Dialog_Result) {
	app := cast(^App)data
	if result.status == .Accepted && len(result.paths) > 0 {
		send(app, Command{op="open", path=result.paths[0]})
		_ = ui.text_buffer_set(&app.search, "")
	}
	if result.status == .Error {
		app.error = "Folder dialog failed"
	}
	alicorn.invalidate_root(rt, "folder dialog completed")
}

smoke_advance :: proc(app: ^App) {
	if !app.smoke {
		return
	}

	switch app.smoke_stage {
	case 0:
		if app.state.ready && app.state.total > 0 && len(app.window.entries) > 0 {
			for row in app.window.entries {
				if !row.directory {
					send(
						app,
						Command{
							op="select",
							id=row.id,
							index=row.index,
							listing_revision=app.state.listing_revision,
							audition=true,
						},
					)
					app.smoke_stage = 1
					break
				}
			}
		}
	case 1:
		if len(app.state.selected.id) > 0 &&
			(!app.smoke_audio || app.state.playback.status == "playing") {
			if app.smoke_audio {
				app.smoke_stage = 2
			} else {
				send(app, Command{op="stop"})
				app.smoke_stage = 3
			}
		}
	case 2:
		if app.state.playback.position > 0 {
			send(app, Command{op="stop"})
			app.smoke_stage = 3
		}
	case 3:
		if app.state.playback.status == "idle" {
			send(app, Command{op="search", query=app.state.selected.name})
			app.smoke_stage = 4
		}
	case 4:
		if !app.state.searching && len(app.state.query) > 0 && app.state.total > 0 {
			app.smoke_stage = 5
			app.idle_builds = app.builds
			app.idle_revision = app.revision
			_ = host.application_schedule_after(app.services.scheduler, .Opportunistic, 500_000_000)
		}
	}
}

wake :: proc(data: rawptr, rt: ^alicorn.Runtime) {
	app := cast(^App)data
	if read_state(app) {
		if app.keyboard_selection && app.state.selected_index >= 0 {
			_ = alicorn.scroll_region_set_offset(
				rt,
				app.list_node,
				max(0, f32(app.state.selected_index-4)*ROW_HEIGHT),
			)
			app.keyboard_selection = false
		}
		alicorn.invalidate_root(rt, "Punks state published")
		smoke_advance(app)
	}
}

scheduled :: proc(data: rawptr, rt: ^alicorn.Runtime, class: host.Scheduled_Wake_Class) {
	app := cast(^App)data
	if class == .Frequent && app.pending_search {
		app.pending_search = false
		send(app, Command{op="search", query=app.search.text})
	}
	if class == .Opportunistic && app.smoke_stage == 5 {
		app.smoke_complete =
			app.state.playback.status == "idle" &&
			app.revision == app.idle_revision &&
			app.builds <= app.idle_builds+1
		host.application_request_quit(app.services.quit)
	} else if class == .Opportunistic && app.smoke {
		host.application_request_quit(app.services.quit)
	}
}

keys :: proc(data: rawptr, rt: ^alicorn.Runtime, key: host.Application_Key) -> bool {
	app := cast(^App)data
	#partial switch key {
	case .Up, .Down:
		if rt.focused == app.search_node {
			return false
		}
		delta: i32 = -1 if key == .Up else 1
		send(app, Command{op="step", delta=delta, audition=true})
		app.keyboard_selection = true
		return true
	case .Return:
		if rt.focused == app.search_node {
			return false
		}
		if len(app.state.selected.id) > 0 {
			send(
				app,
				Command{
					op="enter",
					id=app.state.selected.id,
					index=app.state.selected.index,
					listing_revision=app.state.listing_revision,
					audition=true,
				},
			)
		}
		return true
	case .Toggle:
		if rt.focused == app.search_node {
			return false
		}
		playback_op := "play" if app.state.playback.status == "idle" else "stop"
		send(app, Command{op=playback_op})
		return true
	case .Open_Repository:
		open_folder(app)
		return true
	case .Escape:
		_ = ui.text_buffer_set(&app.search, "")
		send(app, Command{op="search"})
		alicorn.invalidate_root(rt, "search cleared")
		return true
	case:
		return false
	}
}

build :: proc(
	data: rawptr,
	rt: ^alicorn.Runtime,
	width, _height: int,
	_dpi: f32,
) -> alicorn.Node_ID {
	app := cast(^App)data
	ctx, needed := alicorn.begin_frame(rt)
	if !needed {
		return 0
	}
	app.builds += 1
	app.viewport_width = width
	if app.theme == 0 {
		app.theme = alicorn.style_theme_register(rt, punks_theme())
	}

	root := alicorn.container_begin(&ctx, .Root, label="punks", style=ui.layout_style(grow=1), color=ui.rgb(0x181B1D))
	if root == 0 {
		alicorn.end_frame(&ctx)
		return 0
	}
	scope := alicorn.style_environment_push(&ctx, alicorn.Style_Environment{theme=app.theme})
	render_workspace(app, rt, &ctx)
	alicorn.style_environment_pop(&ctx, scope)
	alicorn.container_end(&ctx)
	alicorn.end_frame(&ctx)
	return root
}

main :: proc() {
	app := App{requested_offset=-1}
	for arg, index in os.args {
		if arg == "--smoke" {
			app.smoke = true
		}
		if arg == "--smoke-audio" {
			app.smoke = true
			app.smoke_audio = true
		}
		if arg == "--no-audio" {
			app.no_audio = true
		}
		if arg == "--folder" && index+1 < len(os.args) {
			app.initial_path = os.args[index+1]
		}
	}

	app.search, _ = ui.text_buffer("", context.allocator)
	defer ui.text_buffer_destroy(&app.search)
	for index in 0..<2 {
		_ = arena.arena_init_growing(&app.state_memory[index])
		_ = arena.arena_init_growing(&app.entry_memory[index])
	}
	defer {
		for index in 0..<2 {
			arena.arena_destroy(&app.state_memory[index])
			arena.arena_destroy(&app.entry_memory[index])
		}
	}

	host.Run(host.Application{
		state=&app,
		title="Punks",
		width=960,
		height=720,
		build=build,
		on_start=start,
		on_stop=stop,
		on_wake=wake,
		on_services=services,
		on_dialog=dialog,
		on_key=keys,
		on_scheduled_wake=scheduled,
		interaction_update_policy=.Correct_After_Interaction,
	})
}
