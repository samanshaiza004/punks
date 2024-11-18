// src/renderer/src/keybinds/defaults.ts
import { KeyBindingMap } from '../types/keybinds'

export const defaultKeyBindings: KeyBindingMap = {
  NAVIGATE_UP: {
    id: 'NAVIGATE_UP',
    action: 'Navigate Up',
    defaultKeys: [['ArrowUp'], ['k']],
    currentKeys: [['ArrowUp'], ['k']],
    description: 'Move selection up in the file grid'
  },
  NAVIGATE_DOWN: {
    id: 'NAVIGATE_DOWN',
    action: 'Navigate Down',
    defaultKeys: [['ArrowDown'], ['j']],
    currentKeys: [['ArrowDown'], ['j']],
    description: 'Move selection down in the file grid'
  },
  NAVIGATE_LEFT: {
    id: 'NAVIGATE_LEFT',
    action: 'Navigate Left',
    defaultKeys: [['ArrowLeft'], ['h']],
    currentKeys: [['ArrowLeft'], ['h']],
    description: 'Move selection left in the file grid'
  },
  NAVIGATE_RIGHT: {
    id: 'NAVIGATE_RIGHT',
    action: 'Navigate Right',
    defaultKeys: [['ArrowRight'], ['l']],
    currentKeys: [['ArrowRight'], ['l']],
    description: 'Move selection right in the file grid'
  },
  SELECT_ITEM: {
    id: 'SELECT_ITEM',
    action: 'Select Item',
    defaultKeys: [['Enter'], ['Space']],
    currentKeys: [['Enter'], ['Space']],
    description: 'Open selected file or directory'
  },
  GO_BACK: {
    id: 'GO_BACK',
    action: 'Go Back',
    defaultKeys: [
      ['Alt', 'Left'],
      ['Alt', 'h']
    ],
    currentKeys: [
      ['Alt', 'Left'],
      ['Alt', 'h']
    ],
    description: 'Navigate to parent directory'
  },
  CLOSE_TAB: {
    id: 'CLOSE_TAB',
    action: 'Close Tab',
    defaultKeys: [['Control', 'w']],
    currentKeys: [['Control', 'w']],
    description: 'Close the current tab'
  },
  NEW_TAB: {
    id: 'NEW_TAB',
    action: 'New Tab',
    defaultKeys: [['Control', 't']],
    currentKeys: [['Control', 't']],
    description: 'Create a new tab'
  },
  NEXT_TAB: {
    id: 'NEXT_TAB',
    action: 'Next Tab',
    defaultKeys: [['Control', 'Tab']],
    currentKeys: [['Control', 'Tab']],
    description: 'Switch to the next tab'
  },
  PREVIOUS_TAB: {
    id: 'PREVIOUS_TAB',
    action: 'Previous Tab',
    defaultKeys: [['Control', 'Shift', 'Tab']],
    currentKeys: [['Control', 'Shift', 'Tab']],
    description: 'Switch to the previous tab'
  },
  FOCUS_SEARCH: {
    id: 'FOCUS_SEARCH',
    action: 'Focus Search',
    defaultKeys: [['Control', 'l']],
    currentKeys: [['Control', 'l']],
    description: 'Focus the search bar'
  }
}
