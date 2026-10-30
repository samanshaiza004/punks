package main

import alicorn "alicorn:runtime"
import ui "alicorn:ui"

Punks_Relief_Materials :: struct {
	recessed, mounted: alicorn.Material_ID,
}

punks_styles_ensure :: proc(app: ^App, rt: ^alicorn.Runtime) {
	if app.theme != 0 { return }
	app.materials.recessed = alicorn.style_material_register(rt, alicorn.Style_Material{
		kind=.Analytic_Relief, bevel_width=3, bevel_strength=0.95,
		inner_shadow_strength=0.85,
	})
	app.materials.mounted = alicorn.style_material_register(rt, alicorn.Style_Material{
		kind=.Analytic_Relief, bevel_width=2, bevel_strength=0.95,
		inner_shadow_strength=0.5, outer_shadow_strength=0.65, outer_shadow_radius=4,
	})
	app.theme = alicorn.style_theme_register(rt, punks_theme(app.materials))
}

punks_theme :: proc(materials: Punks_Relief_Materials) -> alicorn.Style_Theme {
	theme := alicorn.DEFAULT_STYLE_THEME
	theme.colors[int(alicorn.Style_Color_Role.Window_Background)] = ui.rgb(0x181B1D)
	theme.colors[int(alicorn.Style_Color_Role.Surface)] = ui.rgb(0x22272A)
	theme.colors[int(alicorn.Style_Color_Role.Subtle_Surface)] = ui.rgb(0x2D3336)
	theme.colors[int(alicorn.Style_Color_Role.Editor_Background)] = ui.rgb(0x1B1F22)
	theme.colors[int(alicorn.Style_Color_Role.Text)] = ui.rgb(0xE8E7E1)
	theme.colors[int(alicorn.Style_Color_Role.Muted_Text)] = ui.rgb(0xADB4B5)
	theme.colors[int(alicorn.Style_Color_Role.Accent)] = ui.rgb(0xD5A363)
	theme.colors[int(alicorn.Style_Color_Role.Accent_Hover)] = ui.rgb(0xE4B77F)
	theme.colors[int(alicorn.Style_Color_Role.Accent_Pressed)] = ui.rgb(0xBF8C4E)
	theme.colors[int(alicorn.Style_Color_Role.Accent_Text)] = ui.rgb(0x191C1E)
	theme.colors[int(alicorn.Style_Color_Role.Selection)] = ui.Color{0.835, 0.639, 0.388, 0.24}
	theme.colors[int(alicorn.Style_Color_Role.Focus)] = ui.rgb(0xE4B77F)
	theme.colors[int(alicorn.Style_Color_Role.Border)] = ui.rgb(0x3B4346)
	theme.colors[int(alicorn.Style_Color_Role.Scrollbar_Track)] = ui.rgb(0x1B1F22)
	theme.colors[int(alicorn.Style_Color_Role.Scrollbar_Thumb)] = ui.rgb(0x596367)
	for variant in ([?]alicorn.Button_Variant{.Default, .Primary, .Toolbar, .Tab, .Quiet}) {
		recipe := &theme.button_recipes.recipes[int(variant)]
		radius := f32(3)
		if variant == .Default || variant == .Primary { radius = 4 }
		recipe.shape = alicorn.Surface_Shape{kind=.Rounded_Rectangle, corner_radius=radius}
		if variant == .Tab { recipe.surface_visible = true }
		recipe.rest_surface = {defined=true, material=materials.mounted, physical_height=1.5}
		recipe.pressed_surface = {defined=true, material=materials.recessed, physical_height=-1}
	}
	return theme
}
