package main

import alicorn "alicorn:runtime"
import ui "alicorn:ui"

// One semantic palette drives the workbench, retained controls and audio deck.
punks_theme :: proc() -> alicorn.Style_Theme {
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
	return theme
}
