// src/keybinds/defaults.ts
import { KeyBindingMap } from './types'

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
  FOCUS_SEARCH: {
    id: 'FOCUS_SEARCH',
    action: 'Focus Search',
    defaultKeys: [['Control', 'l']],
    currentKeys: [['Control', 'l']],
    description: 'Focus the search bar'
  }
}
