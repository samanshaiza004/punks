// src/keybinds/store.ts
import { KeyBindingMap, KeyCombo, KeyBinding } from './types'
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
        this.bindings = saved
      }
    } catch (error) {
      console.error('Failed to load key bindings:', error)
    }
  }

  getBinding(action: string): KeyBinding | undefined {
    return this.bindings[action] // Return undefined if the action doesn't exist
  }

  getAllBindings(): KeyBindingMap {
    return this.bindings
  }

  async updateBinding(action: string, newKeys: KeyCombo[]): Promise<void> {
    if (this.bindings[action]) {
      this.bindings[action].currentKeys = newKeys
      await this.save()
    } else {
      console.warn(`No key binding found for action: ${action}`)
    }
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
