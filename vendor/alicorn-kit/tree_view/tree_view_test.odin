package tree_view

import "core:testing"
import alicorn "alicorn:runtime"
import ui "alicorn:ui"
import model "alikit:tree_model"

@(private)
tree_view_test_items :: [3]Item{
	{ id=Item_ID(101), label="src", expandable=true, expanded=true },
	{ id=Item_ID(102), parent_id=Item_ID(101), label="main.odin", depth=1 },
	{ id=Item_ID(103), label="disabled", disabled=true },
}

@(private)
tree_view_test_describe :: proc(rt: ^alicorn.Runtime, state: ^State, items: []Item, options: Options = Options{selection_enabled=true}) {
	alicorn.invalidate_root(rt, "AliKit tree view test description")
	ctx, should_build := alicorn.begin_frame(rt)
	if !should_build { return }
	_ = alicorn.container_begin(&ctx, .Root,
		key=alicorn.key_string("alikit-tree-test-root"),
		style=alicorn.layout_style(.Column, width=420, height=280, padding=12))
	window := Window{total_count=len(items), revision=1, items=items}
	_ = build(&ctx, state, window, alicorn.key_string("alikit-tree-test"), options)
	alicorn.container_end(&ctx)
	alicorn.end_frame(&ctx)
}

@(test)
test_tree_view_accepts_application_semantic_namespace :: proc(t: ^testing.T) {
	rt := alicorn.new_runtime(alicorn.Rect{0, 0, 480, 320})
	defer alicorn.destroy_runtime(&rt)
	state: State
	items := tree_view_test_items
	tree_view_test_describe(&rt, &state, items[:], Options{semantic_namespace=0x5452454556494557})
	item, found := alicorn.semantic_node_lookup(&rt,
		alicorn.Semantic_ID{namespace=0x5452454556494557, value=u64(Item_ID(101))})
	testing.expect(t, state.semantic_namespace == 0x5452454556494557 && found && item.role == .Tree_Item,
		"applications should be able to preserve stable semantic identities while adopting the generic tree view")
}

@(test)
test_tree_view_publishes_tree_semantics_and_routes_keyboard_focus :: proc(t: ^testing.T) {
	rt := alicorn.new_runtime(alicorn.Rect{0, 0, 480, 320})
	defer alicorn.destroy_runtime(&rt)
	state: State
	items := tree_view_test_items
	tree_view_test_describe(&rt, &state, items[:])
	root_semantic, root_found := alicorn.semantic_node_lookup(&rt,
		alicorn.Semantic_ID{namespace=alicorn.ALICORN_SEMANTIC_NAMESPACE, value=u64(state.tree_node)})
	item_semantic, item_found := alicorn.semantic_node_lookup(&rt,
		alicorn.Semantic_ID{namespace=state.semantic_namespace, value=u64(Item_ID(102))})
	testing.expect(t, root_found && root_semantic.role == .Tree, "the view should publish a Tree semantic root")
	testing.expect(t, item_found && item_semantic.role == .Tree_Item && item_semantic.label == "main.odin" && item_semantic.has_bounds,
		"visible rows should publish stable Tree_Item semantics and finalized bounds")
	testing.expect(t, alicorn.focus(&rt, state.scroll_node), "the virtual tree should expose a focusable keyboard owner")
	handled, event := handle_key(&state, &rt, Window{total_count=len(items), revision=1, items=items[:]}, .Down,
		Options{selection_enabled=true, select_on_focus=true})
	testing.expect(t, handled && event.kind == .Focus_Changed && event.id == Item_ID(101), "Down should route through the component navigation model")
	testing.expect(t, state.model.focused_id == Item_ID(101) && state.model.selected_id == Item_ID(101),
		"focus and optional selection should remain separately represented in state")
	semantic_focus := alicorn.semantic_focus_state(&rt)
	testing.expect(t, semantic_focus.id.value == u64(Item_ID(101)) && semantic_focus.owner == state.scroll_node,
		"keyboard navigation should publish logical semantic focus on the durable list owner")
}

@(test)
test_tree_view_context_menu_targets_stable_item_and_anchors_to_row :: proc(t: ^testing.T) {
	rt := alicorn.new_runtime(alicorn.Rect{0, 0, 480, 320})
	defer alicorn.destroy_runtime(&rt)
	state: State
	items := tree_view_test_items
	tree_view_test_describe(&rt, &state, items[:])
	sid := alicorn.Semantic_ID{namespace=state.semantic_namespace, value=u64(Item_ID(102))}
	row, found := alicorn.semantic_node_lookup(&rt, sid)
	testing.expect(t, found && row.has_bounds, "the target row should have finalized semantic bounds before context input")
	if !found { return }
	target := row.visual_node
	handled, event := pointer(&state, &rt,
		Window{total_count=len(items), revision=1, items=items[:]},
		alicorn.Pointer_Event{kind=.Down, button=alicorn.POINTER_BUTTON_SECONDARY},
		target,
		Options{context_menu_enabled=true, selection_enabled=true})
	testing.expect(t, handled && event.kind == .Context_Menu_Requested && event.id == Item_ID(102),
		"secondary click should return the stable row identity to application-owned menu logic")
	testing.expect(t, alicorn.context_menu_is_open(&rt) && context_target(&state) == Item_ID(102),
		"the helper should open Alicorn's context menu and retain its target identity")
	anchor := context_anchor(&state)
	testing.expect(t, anchor == row.bounds, "the context menu should anchor to the realized row bounds")
}

@(test)
test_tree_view_keeps_paged_rows_bounded :: proc(t: ^testing.T) {
	rt := alicorn.new_runtime(alicorn.Rect{0, 0, 320, 180})
	defer alicorn.destroy_runtime(&rt)
	state: State
	items := [2]Item{
		{ id=Item_ID(402), label="loaded A" },
		{ id=Item_ID(403), label="loaded B" },
	}
	event := tree_view_test_describe_window(&rt, &state, Window{total_count=1000, offset=400, revision=7, items=items[:]})
	testing.expect(t, state.scroll_node != 0 && state.model.revision == 7,
		"the view should accept a bounded data window for a large logical tree")
	testing.expect(t, event.kind == .Range_Requested && event.index == 0,
		"the view should request a visible but unloaded range from the application data provider")
	info, found := alicorn.node_info(&rt, state.scroll_node)
	testing.expect(t, found && info.scroll_content_height >= 1000*DEFAULT_OPTIONS.row_height,
		"the logical list height should reflect the full row count without materializing unloaded items")
}

@(private)
tree_view_test_describe_window :: proc(rt: ^alicorn.Runtime, state: ^State, window: Window) -> Event {
	alicorn.invalidate_root(rt, "AliKit paged tree view test description")
	ctx, should_build := alicorn.begin_frame(rt)
	if !should_build { return {} }
	_ = alicorn.container_begin(&ctx, .Root,
		key=alicorn.key_string("alikit-tree-window-test-root"),
		style=alicorn.layout_style(.Column, width=320, height=180, padding=8))
	event := build(&ctx, state, window, alicorn.key_string("alikit-tree-window-test"))
	alicorn.container_end(&ctx)
	alicorn.end_frame(&ctx)
	return event
}
