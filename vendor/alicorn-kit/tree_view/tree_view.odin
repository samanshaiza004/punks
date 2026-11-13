package tree_view

import "core:math"
import alicorn "alicorn:runtime"
import ui "alicorn:ui"
import model "alikit:tree_model"

Item_ID :: model.Item_ID
Item :: model.Item
Window :: model.Window
State_Model :: model.State
Event :: model.Event
Event_Kind :: model.Event_Kind
Label_Color_Proc :: proc(data: rawptr, ctx: ^ui.UI, item: Item) -> alicorn.Color
Row_Hook_Proc :: proc(data: rawptr, ctx: ^ui.UI, item: Item, semantic: alicorn.Semantic_ID)
Label_Paint_Proc :: proc(data: rawptr, ctx: ^ui.UI, item: Item, label_node: alicorn.Node_ID)

State :: struct {
	model:              model.State,
	tree_node:          alicorn.Node_ID,
	scroll_node:        alicorn.Node_ID,
	semantic_namespace: u64,
	context_target:     Item_ID,
	context_anchor:     alicorn.Rect,
}

Options :: struct {
	accessible_label:   string,
	empty_label:        string,
	loading_label:      string,
	row_height:         f32,
	row_padding:        f32,
	row_gap:            f32,
	indent_width:       f32,
	chevron_size:       f32,
	icon_size:          f32,
	semantic_namespace:     u64,
	scroll_restore:       alicorn.Scroll_Restore_Request,
	icon_proc:          proc(data: rawptr, item: Item) -> alicorn.Icon_ID,
	label_color_proc:   Label_Color_Proc,
	row_hook_proc:      Row_Hook_Proc,
	label_paint_proc:   Label_Paint_Proc,
	callback_data:      rawptr,
	hide_disclosure:    bool,
	selection_enabled:  bool,
	select_on_focus:    bool,
	context_menu_enabled: bool,
}

DEFAULT_OPTIONS :: Options{
	accessible_label="Items",
	empty_label="No items",
	loading_label="Loading…",
	row_height=28,
	row_padding=3,
	row_gap=5,
	indent_width=16,
	chevron_size=14,
	icon_size=16,
}

@(private)
options_resolve :: proc(options: Options) -> Options {
	result := options
	if result.accessible_label == "" { result.accessible_label = DEFAULT_OPTIONS.accessible_label }
	if result.empty_label == "" { result.empty_label = DEFAULT_OPTIONS.empty_label }
	if result.loading_label == "" { result.loading_label = DEFAULT_OPTIONS.loading_label }
	if result.row_height <= 0 || math.is_nan(result.row_height) || math.is_inf(result.row_height) {
		result.row_height = DEFAULT_OPTIONS.row_height
	}
	if result.row_padding < 0 || math.is_nan(result.row_padding) || math.is_inf(result.row_padding) {
		result.row_padding = DEFAULT_OPTIONS.row_padding
	}
	if result.row_gap < 0 || math.is_nan(result.row_gap) || math.is_inf(result.row_gap) {
		result.row_gap = DEFAULT_OPTIONS.row_gap
	}
	if result.indent_width <= 0 || math.is_nan(result.indent_width) || math.is_inf(result.indent_width) {
		result.indent_width = DEFAULT_OPTIONS.indent_width
	}
	if result.chevron_size <= 0 || math.is_nan(result.chevron_size) || math.is_inf(result.chevron_size) {
		result.chevron_size = DEFAULT_OPTIONS.chevron_size
	}
	if result.icon_size <= 0 || math.is_nan(result.icon_size) || math.is_inf(result.icon_size) {
		result.icon_size = DEFAULT_OPTIONS.icon_size
	}
	return result
}

@(private)
semantic_id :: proc(state: ^State, id: Item_ID) -> alicorn.Semantic_ID {
	if state == nil || state.semantic_namespace == 0 || id == Item_ID(0) { return {} }
	return alicorn.Semantic_ID{namespace=state.semantic_namespace, value=u64(id)}
}

@(private)
focus_runtime :: proc(state: ^State, rt: ^alicorn.Runtime, id: Item_ID, index: int) {
	if state == nil || rt == nil { return }
	if state.scroll_node != 0 { _ = alicorn.focus(rt, state.scroll_node) }
	if sid := semantic_id(state, id); sid.namespace != 0 {
		_ = alicorn.semantic_focus_set(rt, sid, state.scroll_node)
	}
	if state.scroll_node != 0 && index >= 0 {
		_ = alicorn.virtual_list_ensure_visible(rt, state.scroll_node, index, "AliKit tree focus moved")
	}
}

@(private)
focus_event :: proc(state: ^State, rt: ^alicorn.Runtime, event: Event) {
	if event.kind == .Focus_Changed { focus_runtime(state, rt, event.id, event.index) }
}

// build renders a borrowed visible-row window using Alicorn's retained
// fixed-row virtual list. Expand/collapse, activation and loading remain
// application events; the component never owns the hierarchy or data source.
build :: proc(ctx: ^ui.UI, state: ^State, window: Window, key: ui.UI_Key, options: Options = {}) -> Event {
	if ctx == nil || ctx.runtime == nil || state == nil { return {} }
	resolved := options_resolve(options)
	if !ui.key_scope(ctx, key) { return {} }
	if window.total_count < 0 || window.offset < 0 || len(window.items) > window.total_count-window.offset {
		_ = ui.text(ctx, "Tree data unavailable", key=alicorn.key_string("tree-data-error"))
		return {}
	}
	model.reconcile(&state.model, window)
	state.tree_node = alicorn.container_begin(ctx, .Container,
		label=resolved.accessible_label,
		key=alicorn.key_string("tree-root"),
		style=ui.layout_style(.Column, grow=1, clip=true),
	)
	if state.tree_node == 0 { return {} }
	state.semantic_namespace = resolved.semantic_namespace
	if state.semantic_namespace == 0 {
		state.semantic_namespace = u64(state.tree_node) ~ 0x414C494B49545245
		if state.semantic_namespace == 0 { state.semantic_namespace = 1 }
	}
	if state.model.focused_id == 0 {
		semantic_focus := alicorn.semantic_focus_state(ctx.runtime)
		if semantic_focus.id.namespace == state.semantic_namespace {
			_ = model.focus_item(&state.model, window, Item_ID(semantic_focus.id.value), select=resolved.select_on_focus)
		}
	}
	_ = alicorn.semantic_description(ctx, .Tree, resolved.accessible_label)
	if window.total_count == 0 {
		_ = ui.text(ctx, resolved.empty_label, key=alicorn.key_string("tree-empty"))
		alicorn.container_end(ctx)
		return {}
	}
	list := alicorn.virtual_list_begin(
		ctx,
		window.total_count,
		resolved.row_height,
		key=alicorn.key_string("tree-rows"),
		style=ui.layout_style(.Column, grow=1, clip=true),
		label=resolved.accessible_label,
		focusable=true,
		restore=resolved.scroll_restore,
	)
	state.scroll_node = list.scroll.id
	event: Event
	if list.scroll.id != 0 {
		for absolute_index := list.first; absolute_index < list.last; absolute_index += 1 {
			local_index := absolute_index-window.offset
			if local_index < 0 || local_index >= len(window.items) {
				if event.kind == .None {
					event = Event{kind=.Range_Requested, index=absolute_index}
				}
				_ = ui.text(ctx, resolved.loading_label,
					key=alicorn.key_pair(0xA11C_7E10, u64(absolute_index)),
					style=ui.layout_style(height=resolved.row_height))
				continue
			}
			item := window.items[local_index]
			row_event := row_build(ctx, state, item, absolute_index, resolved)
			if row_event.kind != .None { event = row_event }
		}
		alicorn.virtual_list_end(ctx, list)
	}
	if event.kind == .None && state.model.pending_focus &&
		(state.model.pending_index < window.offset || state.model.pending_index >= window.offset+len(window.items)) {
		event = Event{kind=.Range_Requested, index=state.model.pending_index}
		_ = alicorn.virtual_list_ensure_visible(ctx.runtime, state.scroll_node, event.index, "AliKit tree pending focus")
	}
	alicorn.container_end(ctx)
	if event.kind == .Pressed {
		focus_runtime(state, ctx.runtime, event.id, event.index)
		alicorn.invalidate_root(ctx.runtime, "AliKit tree row pressed")
	}
	return event
}

@(private)
row_build :: proc(ctx: ^ui.UI, state: ^State, item: Item, index: int, options: Options) -> Event {
	selected := state.model.selected_id == item.id
	focused := state.model.focused_id == item.id
	button, pressed := alicorn.button_begin(
		ctx,
		item.label,
		key=alicorn.key_pair(0xA11C_7E11, u64(item.id)),
		style=ui.layout_style(.Row, height=options.row_height, padding=options.row_padding, gap=options.row_gap, align=.Center),
		state=alicorn.Button_State{selected=selected || focused, disabled=item.disabled},
		text_style=alicorn.Text_Style{overflow=.Ellipsis},
		content_style=alicorn.button_content_style(horizontal=.Start, vertical=.Center, padding_x=6, padding_y=2),
		variant=.Quiet,
		presentation=.Custom,
	)
	if button == 0 { return {} }
	states := alicorn.Semantic_States{}
	actions := alicorn.Semantic_Actions{}
	if item.disabled { states = alicorn.semantic_states_add(states, .Disabled) }
	if selected { states = alicorn.semantic_states_add(states, .Selected) }
	if focused { states = alicorn.semantic_states_add(states, .Current) }
	if item.expanded { states = alicorn.semantic_states_add(states, .Expanded) }
	if !item.disabled {
		actions = alicorn.semantic_actions_add(actions, .Press)
		actions = alicorn.semantic_actions_add(actions, .Focus)
		actions = alicorn.semantic_actions_add(actions, .Select)
		if item.expandable {
			actions = alicorn.semantic_actions_add(actions, .Collapse if item.expanded else .Expand)
		}
	}
	description := "Tree item"
	if item.expandable { description = "Collapsed item" if !item.expanded else "Expanded item" }
	_ = alicorn.semantic_description(ctx, .Tree_Item, item.label, description=description, states=states, actions=actions)
	_ = alicorn.semantic_bind(ctx, semantic_id(state, item.id))
	if item.depth > 0 {
		if ui.column(ctx, key=alicorn.key_string("tree-indent"), width=f32(item.depth)*options.indent_width, height=1, label="tree-indent") {}
	}
	if !options.hide_disclosure {
		if item.expandable {
			icon := alicorn.Icon_ID.Chevron_Right
			if item.expanded { icon = .Chevron_Down }
			_ = ui.icon(ctx, icon, key=alicorn.key_string("tree-disclosure"), size=options.chevron_size)
		} else {
			if ui.column(ctx, key=alicorn.key_string("tree-disclosure-space"), width=options.chevron_size, height=options.chevron_size, label="tree-disclosure-space") {}
		}
	}
	if options.icon_proc != nil {
		if icon := options.icon_proc(options.callback_data, item); icon != .None {
			_ = ui.icon(ctx, icon, key=alicorn.key_string("tree-item-icon"), size=options.icon_size)
		}
	}
	if options.row_hook_proc != nil {
		options.row_hook_proc(options.callback_data, ctx, item, semantic_id(state, item.id))
	}
	label_color := alicorn.style_color(ctx, .Text)
	if options.label_color_proc != nil { label_color = options.label_color_proc(options.callback_data, ctx, item) }
	label_node := ui.text(ctx, item.label,
		key=alicorn.key_string("tree-label"),
		style=ui.layout_style(.Row, grow=1),
		text_style=alicorn.Text_Style{overflow=.Ellipsis},
		color=label_color,
	)
	if options.label_paint_proc != nil && label_node != 0 {
		options.label_paint_proc(options.callback_data, ctx, item, label_node)
	}
	alicorn.button_end(ctx)
	if pressed {
		return model.press(&state.model, Window{total_count=index+1, offset=index, revision=state.model.revision, items=[]Item{item}}, item.id, select=options.selection_enabled)
	}
	return {}
}

// handle_key routes only when this tree owns Alicorn's keyboard focus. It maps
// platform-neutral Alicorn keys into the UI-independent navigation model.
handle_key :: proc(
	state: ^State,
	rt: ^alicorn.Runtime,
	window: Window,
	key: alicorn.Application_Key,
	options: Options = {},
) -> (handled: bool, event: Event) {
	if state == nil || rt == nil || state.scroll_node == 0 || alicorn.focused_node(rt) != state.scroll_node { return }
	model_key: model.Key
	#partial switch key {
	case .Up: model_key = .Up
	case .Down: model_key = .Down
	case .Home: model_key = .Home
	case .End: model_key = .End
	case .Page_Up: model_key = .Page_Up
	case .Page_Down: model_key = .Page_Down
	case .Left: model_key = .Left
	case .Right: model_key = .Right
	case .Return: model_key = .Enter
	case .Context_Menu: model_key = .Context_Menu
	case: return
	}
	resolved := options_resolve(options)
	page_size := 10
	if info, found := alicorn.node_info(rt, state.scroll_node); found && info.scroll_viewport_bounds.h > 0 {
		page_size = max(1, int(info.scroll_viewport_bounds.h/resolved.row_height)-1)
	}
	event = model.handle_key(&state.model, window, model_key, page_size=page_size, select_on_focus=resolved.select_on_focus)
	handled = true
	if event.kind == .Context_Menu_Requested {
		if !resolved.context_menu_enabled || !open_context_menu(state, rt, window, event.id) {
			event = Event{}
			return false, event
		}
	}
	if event.kind == .Focus_Changed || event.kind == .Range_Requested {
		index := event.index
		if event.kind == .Focus_Changed { focus_event(state, rt, event) }
		if index >= 0 { _ = alicorn.virtual_list_ensure_visible(rt, state.scroll_node, index, "AliKit tree navigation target") }
		alicorn.invalidate_root(rt, "AliKit tree keyboard navigation")
	} else if event.kind == .Expand_Requested || event.kind == .Collapse_Requested || event.kind == .Activated {
		alicorn.invalidate_root(rt, "AliKit tree keyboard action")
	}
	return
}

// pointer handles secondary-click context menus. The caller supplies the
// already hit-tested node from Alicorn's normal pointer callback.
pointer :: proc(
	state: ^State,
	rt: ^alicorn.Runtime,
	window: Window,
	event: alicorn.Pointer_Event,
	target: alicorn.Node_ID,
	options: Options = {},
) -> (handled: bool, result: Event) {
	resolved := options_resolve(options)
	if !resolved.context_menu_enabled || state == nil || rt == nil || event.kind != .Down ||
		event.button != alicorn.POINTER_BUTTON_SECONDARY || target == 0 { return }
	info, found := alicorn.node_info(rt, target)
	if !found || info.semantic_id.namespace != state.semantic_namespace { return }
	id := Item_ID(info.semantic_id.value)
	if index := tree_model_index(window, id); index < 0 || window.items[index].disabled { return }
	result = model.press(&state.model, window, id, select=resolved.selection_enabled)
	if result.kind == .None || !open_context_menu(state, rt, window, id) { return false, {} }
	result = Event{kind=.Context_Menu_Requested, id=id, index=state.model.focused_index}
	return true, result
}

@(private)
tree_model_index :: proc(window: Window, id: Item_ID) -> int {
	for item, index in window.items { if item.id == id { return index } }
	return -1
}

@(private)
open_context_menu :: proc(state: ^State, rt: ^alicorn.Runtime, window: Window, id: Item_ID, width := alicorn.CONTEXT_MENU_DEFAULT_WIDTH) -> bool {
	if state == nil || rt == nil || state.scroll_node == 0 { return false }
	index := tree_model_index(window, id)
	if index < 0 || window.items[index].disabled { return false }
	_ = model.focus_item(&state.model, window, id)
	focus_runtime(state, rt, id, state.model.focused_index)
	sid := semantic_id(state, id)
	node, found := alicorn.semantic_node_lookup(rt, sid)
	if !found || !node.has_bounds { return false }
	state.context_target = id
	state.context_anchor = node.bounds
	return alicorn.context_menu_open(rt, node.bounds, restore_focus_to=state.scroll_node, width=width)
}

context_target :: proc(state: ^State) -> Item_ID {
	if state == nil { return 0 }
	return state.context_target
}

context_anchor :: proc(state: ^State) -> alicorn.Rect {
	if state == nil { return {} }
	return state.context_anchor
}
