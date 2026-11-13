package tree_model

import "core:testing"
import "core:fmt"

TREE_ROWS :: [5]Item{
	{ id=Item_ID(1), label="src", expandable=true, expanded=true },
	{ id=Item_ID(2), parent_id=Item_ID(1), label="main.odin", depth=1 },
	{ id=Item_ID(3), parent_id=Item_ID(1), label="tests", depth=1, expandable=true },
	{ id=Item_ID(4), parent_id=Item_ID(3), label="main_test.odin", depth=2 },
	{ id=Item_ID(5), label="README.md", disabled=true },
}

@(test)
test_tree_navigation_expansion_and_parent_focus :: proc(t: ^testing.T) {
	items := TREE_ROWS
	window := Window{total_count=5, revision=1, items=items[:]}
	state: State
	event := handle_key(&state, window, .Down)
	testing.expect(t, event.kind == .Focus_Changed && event.id == Item_ID(1), fmt.tprintf("Down without a current item should focus the first enabled row (kind=%v id=%d focus=%d index=%d)", event.kind, u64(event.id), u64(state.focused_id), state.focused_index))
	event = handle_key(&state, window, .Right)
	testing.expect(t, event.kind == .Focus_Changed && event.id == Item_ID(2), "Right on an expanded item should focus its first child")
	event = handle_key(&state, window, .Down)
	testing.expect(t, event.kind == .Focus_Changed && event.id == Item_ID(3), "Down should move through visible rows by stable identity")
	event = handle_key(&state, window, .Right)
	testing.expect(t, event.kind == .Expand_Requested && event.id == Item_ID(3), "Right on a collapsed expandable row should request expansion")
	event = handle_key(&state, window, .Left)
	testing.expect(t, event.kind == .Focus_Changed && event.id == Item_ID(1), "Left on a collapsed child should focus its visible parent")
	event = handle_key(&state, window, .Enter)
	testing.expect(t, event.kind == .Activated && event.id == Item_ID(1), "Enter should activate the focused stable identity")
}

@(test)
test_right_skips_disabled_direct_children :: proc(t: ^testing.T) {
	items := [4]Item{
		{ id=Item_ID(20), label="root", expandable=true, expanded=true },
		{ id=Item_ID(21), parent_id=Item_ID(20), label="disabled child", depth=1, expandable=true, expanded=true, disabled=true },
		{ id=Item_ID(22), parent_id=Item_ID(21), label="nested", depth=2 },
		{ id=Item_ID(23), parent_id=Item_ID(20), label="enabled child", depth=1 },
	}
	window := Window{total_count=len(items), revision=1, items=items[:]}
	state: State
	_ = handle_key(&state, window, .Home)
	event := handle_key(&state, window, .Right)
	testing.expect(t, event.kind == .Focus_Changed && event.id == Item_ID(23),
		"Right should skip disabled children and their descendants to focus the next enabled direct child")
}

@(test)
test_disabled_rows_and_optional_selection :: proc(t: ^testing.T) {
	items := TREE_ROWS
	window := Window{total_count=5, revision=1, items=items[:]}
	state: State
	_ = handle_key(&state, window, .Home)
	testing.expect(t, state.focused_id == Item_ID(1) && state.selected_id == 0, "focus should remain separate from application selection")
	event := handle_key(&state, window, .End)
	testing.expect(t, event.kind == .Focus_Changed && state.focused_id == Item_ID(4), "End should skip a disabled last row and focus the last enabled row")
	pressed := press(&state, window, Item_ID(5))
	testing.expect(t, pressed.kind == .None && state.focused_id == Item_ID(4), "disabled rows must not receive pointer activation or focus")
	pressed = press(&state, window, Item_ID(2))
	testing.expect(t, pressed.kind == .Pressed && state.focused_id == Item_ID(2) && state.selected_id == Item_ID(2), "press should focus and select by stable ID")
}

@(test)
test_reconcile_preserves_identity_when_rows_reorder :: proc(t: ^testing.T) {
	items := TREE_ROWS
	window := Window{total_count=3, revision=1, items=items[:3]}
	state := State{focused_id=Item_ID(2), selected_id=Item_ID(2), focused_index=1, revision=1}
	reordered := [3]Item{items[2], items[0], items[1]}
	next := Window{total_count=3, revision=2, items=reordered[:]}
	reconcile(&state, next)
	testing.expect(t, state.focused_id == Item_ID(2) && state.focused_index == 2, "reordering should preserve the focused item identity and update its index")
	testing.expect(t, state.selected_id == Item_ID(2), "reordering should preserve the selected item identity")
}

@(test)
test_navigation_requests_missing_window_and_resolves_pending_focus :: proc(t: ^testing.T) {
	first_rows := [2]Item{
		{ id=Item_ID(10), label="one" },
		{ id=Item_ID(11), label="two" },
	}
	first := Window{total_count=4, offset=0, revision=8, items=first_rows[:]}
	state := State{focused_id=Item_ID(11), focused_index=1, revision=8}
	event := handle_key(&state, first, .Down)
	testing.expect(t, event.kind == .Range_Requested && event.index == 2 && state.focused_id == Item_ID(11), "moving beyond a loaded window should request the missing row and retain the current focus")
	second_rows := [2]Item{
		{ id=Item_ID(12), label="three" },
		{ id=Item_ID(13), label="four" },
	}
	second := Window{total_count=4, offset=2, revision=8, items=second_rows[:]}
	reconcile(&state, second)
	testing.expect(t, state.focused_id == Item_ID(12) && state.focused_index == 2 && state.pending_index == -1, "publishing the requested window should complete the pending focus move")
}

@(test)
test_pending_focus_skips_disabled_loaded_rows :: proc(t: ^testing.T) {
	state := State{focused_id=Item_ID(31), focused_index=1, pending_index=2, pending_focus=true, revision=4}
	rows := [2]Item{
		{id=Item_ID(32), label="disabled", disabled=true},
		{id=Item_ID(33), label="enabled"},
	}
	reconcile(&state, Window{total_count=4, offset=2, revision=4, items=rows[:]})
	testing.expect(t, state.focused_id == Item_ID(33) && state.focused_index == 3 && !state.pending_focus,
		"a loaded disabled row should not strand a pending keyboard focus move")
}

@(test)
test_removed_focus_reconciles_to_nearest_enabled_row :: proc(t: ^testing.T) {
	state := State{focused_id=Item_ID(41), focused_index=1, selected_id=Item_ID(41), revision=1}
	next := [3]Item{
		{id=Item_ID(40), label="before"},
		{id=Item_ID(43), label="disabled replacement", disabled=true},
		{id=Item_ID(42), label="after", disabled=true},
	}
	reconcile(&state, Window{total_count=3, revision=2, items=next[:]})
	testing.expect(t, state.focused_id == Item_ID(40) && state.focused_index == 0,
		"when a focused row disappears, reconciliation should choose the nearest enabled item rather than retain a stale ID")
	testing.expect(t, state.selected_id == 0,
		"a removed selected row should be cleared during complete-projection reconciliation")
}

@(test)
test_window_validation_rejects_invalid_ranges_and_duplicate_ids :: proc(t: ^testing.T) {
	rows := [2]Item{{id=Item_ID(1), label="one"}, {id=Item_ID(1), label="duplicate"}}
	status := validate(Window{total_count=2, offset=0, revision=1, items=rows[:]})
	testing.expect(t, status == .Duplicate_ID, "visible row IDs must be unique")
	status = validate(Window{total_count=2, offset=2, revision=1, items=rows[:]})
	testing.expect(t, status == .Invalid_Window, "window data must fit inside the logical row count")
}
