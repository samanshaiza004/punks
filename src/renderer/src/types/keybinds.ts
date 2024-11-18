export type KeyCombo = string[]

export interface KeyBinding {
  id: string
  action: string
  defaultKeys: KeyCombo[] // Now represents an array of key combinations
  currentKeys: KeyCombo[] // Array of key combinations
  description: string
}

export interface KeyBindingMap {
  [key: string]: KeyBinding
}

export type KeyAction =
  | 'NAVIGATE_UP'
  | 'NAVIGATE_DOWN'
  | 'NAVIGATE_LEFT'
  | 'NAVIGATE_RIGHT'
  | 'SELECT_ITEM'
  | 'GO_BACK'
  | 'FOCUS_SEARCH'
  | 'CLOSE_TAB'
  | 'NEW_TAB'
  | 'NEXT_TAB'
  | 'PREVIOUS_TAB'
  | 'MOVE_TAB_LEFT'
  | 'MOVE_TAB_RIGHT'
  | 'DUPLICATE_TAB'
  | 'CLOSE_OTHER_TABS'

export type KeyHandler = () => void

export interface KeyHandlerMap {
  [key: string]: KeyHandler
}
