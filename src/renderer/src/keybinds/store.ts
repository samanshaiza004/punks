// src/keybinds/store.ts
import { KeyBindingMap, KeyCombo, KeyBinding } from '../types/keybinds'
import { defaultKeyBindings } from './defaults'

export class KeyBindingStore {
  private static instance: KeyBindingStore
  private bindings: KeyBindingMap

  private constructor() {
    this.bindings = defaultKeyBindings
  }

  static getInstance(): KeyBindingStore {
    if (!KeyBindingStore.instance) {
      KeyBindingStore.instance = new KeyBindingStore()
    }
    return KeyBindingStore.instance
  }

  async initialize(): Promise<void> {
    try {
      const saved = await window.api.getKeyBindings()
      if (saved) {
        // Merge saved bindings with defaults to ensure all actions are available
        const mergedBindings: KeyBindingMap = { ...defaultKeyBindings }
        Object.keys(saved).forEach((key) => {
          if (mergedBindings[key]) {
            mergedBindings[key] = {
              ...mergedBindings[key],
              currentKeys: saved[key].currentKeys
            }
          }
        })
        this.bindings = mergedBindings
      } else {
        // If no saved bindings, use defaults
        this.bindings = { ...defaultKeyBindings }
      }
      console.log('Initialized key bindings:', this.bindings)
    } catch (error) {
      console.error('Failed to load key bindings:', error)
      // Fall back to defaults on error
      this.bindings = { ...defaultKeyBindings }
    }
  }

  async updateBinding(action: string, newKeys: KeyCombo[]): Promise<void> {
    if (this.bindings[action]) {
      // Ensure newKeys is always an array of arrays
      this.bindings[action].currentKeys = newKeys.map((combo) =>
        Array.isArray(combo) ? combo : [combo]
      )
      await this.save()
    } else {
      console.warn(`No key binding found for action: ${action}`)
    }
  }

  getBinding(action: string): KeyBinding | undefined {
    return this.bindings[action] // Return undefined if the action doesn't exist
  }

  getAllBindings(): KeyBindingMap {
    return this.bindings
  }

  async resetBinding(action: string): Promise<void> {
    if (this.bindings[action]) {
      this.bindings[action].currentKeys = [...this.bindings[action].defaultKeys]
      await this.save()
    } else {
      console.warn(`No key binding found for action: ${action}`)
    }
  }

  async resetAll(): Promise<void> {
    Object.keys(this.bindings).forEach((action) => {
      this.bindings[action].currentKeys = [...this.bindings[action].defaultKeys]
    })
    await this.save()
  }

  private async save(): Promise<void> {
    try {
      await window.api.saveKeyBindings(this.bindings)
    } catch (error) {
      console.error('Failed to save key bindings:', error)
    }
  }
}
