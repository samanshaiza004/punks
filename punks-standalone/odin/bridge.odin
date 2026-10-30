package main

when ODIN_OS == .Windows {
	foreign import backend "system:punks_caliber.dll.lib"
} else {
	foreign import backend "system:punks_caliber"
}

Status :: distinct i32
OK :: Status(0)
STOPPED :: Status(11)

Publication :: struct {
	revision: u64,
	schema: u32,
	reserved: u32,
	data: [^]u8,
	len: uintptr,
	lease: rawptr,
}

Resource_View :: struct {
	id: u64,
	generation: u64,
	data: [^]u8,
	len: uintptr,
	lease: rawptr,
}

Telemetry_Info :: struct {
	sequence: u64,
	schema: u32,
	reserved: u32,
	count: uintptr,
	value_size: uintptr,
}

Caliber_API :: struct {
	abi_version: u32,
	struct_size: u32,
	context_create: proc "c" (rawptr, ^rawptr) -> Status,
	context_destroy: proc "c" (rawptr),
	dispatch: proc "c" (rawptr, [^]u8, uintptr) -> Status,
	peek: proc "c" (rawptr, ^uintptr) -> Status,
	take: proc "c" (rawptr, [^]u8, uintptr, ^uintptr) -> Status,
	publish_state: proc "c" (rawptr, u32, [^]u8, uintptr, ^u64) -> Status,
	read_state: proc "c" (rawptr, ^Publication) -> Status,
	release_state: proc "c" (^Publication),
	map_resource: proc "c" (rawptr, u64, u64, ^Resource_View) -> Status,
	release_resource_view: proc "c" (^Resource_View),
	publish_resource: proc "c" (rawptr, [^]u8, uintptr, ^u64, ^u64) -> Status,
	retire_resource: proc "c" (rawptr, u64, u64) -> Status,
	publish_telemetry: proc "c" (rawptr, [^]uintptr, uintptr) -> Status,
	read_telemetry: proc "c" (rawptr, [^]uintptr, uintptr, ^Telemetry_Info) -> Status,
	wake_sequence: proc "c" (rawptr, ^u64) -> Status,
	wait_wake: proc "c" (rawptr, u64, ^u64) -> Status,
	stop_waiters: proc "c" (rawptr) -> Status,
}

foreign backend {
	caliber_get_api :: proc(version: u32) -> ^Caliber_API ---
	punks_create :: proc(version, flags: u32, session, transport: ^rawptr) -> Status ---
	punks_dispatch :: proc(session: rawptr, data: [^]u8, len: uintptr) -> Status ---
	punks_stop :: proc(session: rawptr) -> Status ---
	punks_destroy :: proc(session: rawptr) -> Status ---
}

Resource :: struct {
	id: u64,
	generation: u64,
}

Tab_View :: struct {
	id: u64,
	title: string,
}

Entry :: struct {
	id: string,
	index: int,
	name: string,
	path: string,
	format: string,
	size: u64,
	directory: bool,
}

Entry_Window :: struct {
	v: u32,
	listing_revision: u64,
	offset: int,
	total: int,
	entries: []Entry,
}

Folder_Window :: struct {
	v: u32,
	listing_revision: u64,
	offset: int,
	total: int,
	entries: []Entry,
}

Playback :: struct {
	status: string,
	file: string,
	position: f64,
	duration: f64,
}

State :: struct {
	v: u32,
	ready: bool,
	directory: string,
	can_navigate_up: bool,
	query: string,
	searching: bool,
	listing_revision: u64,
	total: int,
	offset: int,
	count: int,
	entries: Resource,
	folders: Resource,
	folder_count: int,
	folder_offset: int,
	breadcrumbs: []string,
	tabs: []Tab_View,
	active_tab: u64,
	selected_index: i64,
	selected: Entry,
	playback: Playback,
	volume: f32,
	audio_available: bool,
	audio_error: string,
	error: string,
	scanning: bool,
	scan_done: int,
	scan_total: int,
	command_sequence: u64,
	last_request_id: u64,
	waveform_file: string,
	waveform_start: f64,
	waveform_duration: f64,
	waveform_peaks: [][2]i16,
}

Command :: struct {
	v: u32,
	op: string,
	path: string,
	query: string,
	id: string,
	index: int,
	listing_revision: u64,
	offset: int,
	delta: i32,
	seconds: f64,
	value: f32,
	audition: bool,
	request_id: u64,
	tab_id: u64,
	tab_target_id: u64,
	after: bool,
}
