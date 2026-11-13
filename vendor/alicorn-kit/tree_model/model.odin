package tree_model

Item_ID :: distinct u64

Item :: struct {
	id:         Item_ID,
	parent_id:  Item_ID,
	label:      string,
	depth:      int,
	expandable: bool,
	expanded:   bool,
	disabled:   bool,
}

// Window is a contiguous borrowed slice of the current visible-row projection.
// total_count is the logical row count and offset is the first supplied row's
// position. Callers with all rows use offset=0; paged consumers can supply only
// the loaded window without materializing the full hierarchy.
Window :: struct {
	total_count: int,
	offset:      int,
	revision:    u64,
	items:       []Item,
}

State :: struct {
	focused_id:   Item_ID,
	selected_id:  Item_ID,
	focused_index: int,
	pending_index: int,
	pending_focus: bool,
	revision:     u64,
}

Key :: enum {
	Up,
	Down,
	Home,
	End,
	Page_Up,
	Page_Down,
	Left,
	Right,
	Enter,
	Context_Menu,
}

Event_Kind :: enum {
	None,
	Focus_Changed,
	Pressed,
	Activated,
	Expand_Requested,
	Collapse_Requested,
	Parent_Focus_Requested,
	Context_Menu_Requested,
	Range_Requested,
}

Event :: struct {
	kind: Event_Kind,
	id:   Item_ID,
	index: int,
}

Validation_Status :: enum {
	Ok,
	Invalid_Window,
	Invalid_ID,
	Duplicate_ID,
	Invalid_Depth,
	Invalid_Expansion,
	Invalid_Parent,
}

validate :: proc(window: Window) -> Validation_Status {
	if window.total_count < 0 || window.offset < 0 || len(window.items) > window.total_count-window.offset {
		return .Invalid_Window
	}
	for item, index in window.items {
		if item.id == Item_ID(0) { return .Invalid_ID }
		if item.depth < 0 { return .Invalid_Depth }
		if item.expanded && !item.expandable { return .Invalid_Expansion }
		if item.parent_id == item.id { return .Invalid_Parent }
		for previous in window.items[:index] {
			if previous.id == item.id { return .Duplicate_ID }
		}
		if item.parent_id != Item_ID(0) {
			if parent_index := index_of(window.items, item.parent_id); parent_index >= 0 {
				if window.items[parent_index].depth >= item.depth { return .Invalid_Parent }
			}
		}
	}
	return .Ok
}

@(private)
index_of :: proc(items: []Item, id: Item_ID) -> int {
	for item, index in items { if item.id == id { return index } }
	return -1
}

@(private)
absolute_index_of :: proc(window: Window, id: Item_ID) -> int {
	index := index_of(window.items, id)
	return window.offset+index if index >= 0 else -1
}

@(private)
item_at :: proc(window: Window, absolute_index: int) -> (item: Item, found: bool) {
	local := absolute_index-window.offset
	if local < 0 || local >= len(window.items) { return }
	return window.items[local], true
}

@(private)
set_focus :: proc(state: ^State, window: Window, item: Item, index: int, select_on_focus: bool) -> Event {
	changed := state.focused_id != item.id
	state.focused_id = item.id
	state.focused_index = index
	state.pending_index = -1
	state.pending_focus = false
	state.revision = window.revision
	if select_on_focus { state.selected_id = item.id }
	if changed { return Event{kind=.Focus_Changed, id=item.id, index=index} }
	return Event{}
}

@(private)
enabled_at_or_after :: proc(window: Window, index: int, direction: int) -> (item: Item, found: bool, resolved_index: int) {
	for current := index; current >= 0 && current < window.total_count; current += direction {
		candidate, loaded := item_at(window, current)
		if !loaded { return {}, false, current }
		if !candidate.disabled { return candidate, true, current }
	}
	return {}, false, -1
}

@(private)
focus_index :: proc(state: ^State, window: Window, index: int, direction: int, select_on_focus: bool) -> Event {
	if index < 0 || index >= window.total_count { return {} }
	candidate, found, resolved_index := enabled_at_or_after(window, index, direction)
	if !found {
		if resolved_index >= 0 {
			state.pending_index = resolved_index
			state.pending_focus = true
			return Event{kind=.Range_Requested, index=resolved_index}
		}
		return {}
	}
	return set_focus(state, window, candidate, resolved_index, select_on_focus)
}

// reconcile keeps focus and selection attached to stable item identities while
// the caller republishes or reorders its visible-row projection. A pending
// keyboard move is completed when the requested index arrives in a later window.
reconcile :: proc(state: ^State, window: Window) {
	if state == nil { return }
	if state.pending_focus && state.pending_index >= window.offset && state.pending_index < window.offset+len(window.items) {
		candidate, found, resolved_index := enabled_at_or_after(window, state.pending_index, 1)
		if found {
			state.focused_id = candidate.id
			state.focused_index = resolved_index
			state.pending_index = -1
			state.pending_focus = false
		} else if resolved_index >= 0 {
			state.pending_index = resolved_index
		} else {
			state.pending_index = -1
			state.pending_focus = false
		}
	}
	if state.focused_id != Item_ID(0) {
		if index := index_of(window.items, state.focused_id); index >= 0 {
			state.focused_index = window.offset+index
		} else if state.revision != window.revision && window.offset == 0 && len(window.items) == window.total_count {
			if window.total_count == 0 {
				state.focused_id = 0
				state.focused_index = -1
			} else {
				origin := min(max(state.focused_index, 0), window.total_count-1)
				found := false
				for distance := 0; distance < window.total_count && !found; distance += 1 {
					candidates := [2]int{origin+distance, origin-distance}
					for candidate_index in candidates {
						if candidate_index < 0 || candidate_index >= window.total_count { continue }
						if candidate, candidate_found := item_at(window, candidate_index); candidate_found && !candidate.disabled {
							state.focused_id = candidate.id
							state.focused_index = candidate_index
							found = true
							break
						}
					}
				}
				if !found {
					state.focused_id = 0
					state.focused_index = -1
				}
			}
		}
	}
	if state.selected_id != Item_ID(0) && state.revision != window.revision && window.offset == 0 &&
		len(window.items) == window.total_count &&
		index_of(window.items, state.selected_id) < 0 {
		state.selected_id = 0
	}
	state.revision = window.revision
}

// focus_item focuses a loaded, enabled item by stable ID. The ID remains
// borrowed only for the duration of this call; State stores the scalar ID.
focus_item :: proc(state: ^State, window: Window, id: Item_ID, select := false) -> Event {
	if state == nil { return {} }
	index := index_of(window.items, id)
	if index < 0 || window.items[index].disabled { return {} }
	return set_focus(state, window, window.items[index], window.offset+index, select)
}

// set_selection changes application-visible single selection independently of
// keyboard focus. Passing zero clears selection.
set_selection :: proc(state: ^State, window: Window, id: Item_ID) -> bool {
	if state == nil { return false }
	if id != Item_ID(0) {
		index := index_of(window.items, id)
		if index < 0 || window.items[index].disabled { return false }
	}
	state.selected_id = id
	state.revision = window.revision
	return true
}

press :: proc(state: ^State, window: Window, id: Item_ID, select := true) -> Event {
	if state == nil { return {} }
	index := index_of(window.items, id)
	if index < 0 || window.items[index].disabled { return {} }
	item := window.items[index]
	state.focused_id = item.id
	state.focused_index = window.offset+index
	if select { state.selected_id = item.id }
	state.pending_index = -1
	state.pending_focus = false
	state.revision = window.revision
	return Event{kind=.Pressed, id=item.id, index=state.focused_index}
}

handle_key :: proc(
	state: ^State,
	window: Window,
	key: Key,
	page_size := 10,
	select_on_focus := false,
) -> Event {
	if state == nil || window.total_count <= 0 { return {} }
	reconcile(state, window)
	current_index := absolute_index_of(window, state.focused_id)
	if state.focused_id != Item_ID(0) && current_index < 0 { current_index = state.focused_index }
	#partial switch key {
	case .Up, .Down, .Home, .End, .Page_Up, .Page_Down:
		next := current_index
		direction := 1
		#partial switch key {
		case .Up: next -= 1; direction = -1
		case .Down: next += 1; direction = 1
		case .Home: next = 0; direction = 1
		case .End: next = window.total_count-1; direction = -1
		case .Page_Up: next -= max(page_size, 1); direction = -1
		case .Page_Down: next += max(page_size, 1); direction = 1
		}
		if current_index < 0 {
			if key == .Up || key == .End { next = window.total_count-1; direction = -1 } else { next = 0; direction = 1 }
		}
		return focus_index(state, window, next, direction, select_on_focus)
	case .Left, .Right:
		item_index := index_of(window.items, state.focused_id)
		if item_index < 0 { return {} }
		item := window.items[item_index]
		if key == .Right {
			if item.expandable && !item.expanded { return Event{kind=.Expand_Requested, id=item.id, index=window.offset+item_index} }
			if item.expanded {
				for child_index := item_index+1; child_index < len(window.items); child_index += 1 {
					child := window.items[child_index]
					if child.depth <= item.depth { break }
					if child.parent_id == item.id && !child.disabled {
						return set_focus(state, window, child, window.offset+child_index, select_on_focus)
					}
				}
				child_index := item_index+1
				if window.offset+child_index >= window.offset+len(window.items) && window.offset+child_index < window.total_count {
					state.pending_index = window.offset+child_index
					state.pending_focus = true
					return Event{kind=.Range_Requested, index=state.pending_index}
				}
			}
		} else {
			if item.expandable && item.expanded { return Event{kind=.Collapse_Requested, id=item.id, index=window.offset+item_index} }
			if item.parent_id != Item_ID(0) {
				if parent_index := index_of(window.items, item.parent_id); parent_index >= 0 {
					return set_focus(state, window, window.items[parent_index], window.offset+parent_index, select_on_focus)
				}
				return Event{kind=.Parent_Focus_Requested, id=item.parent_id}
			}
		}
	case .Enter:
		if item_index := index_of(window.items, state.focused_id); item_index >= 0 && !window.items[item_index].disabled {
			return Event{kind=.Activated, id=state.focused_id, index=window.offset+item_index}
		}
	case .Context_Menu:
		if item_index := index_of(window.items, state.focused_id); item_index >= 0 {
			return Event{kind=.Context_Menu_Requested, id=state.focused_id, index=window.offset+item_index}
		}
	}
	return {}
}
