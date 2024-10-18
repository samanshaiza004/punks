// src/keybinds/hooks.ts
import { useEffect } from 'react'
import { KeyHandlerMap } from './types'
import { KeyBindingStore } from './store'

export function useKeyBindings(handlers: KeyHandlerMap, enabled = true) {
  const store = KeyBindingStore.getInstance()

  useEffect(() => {
    if (!enabled) return

    const handleKeyDown = (event: KeyboardEvent) => {
      // Ignore key events when typing in input fields
      if (event.target instanceof HTMLInputElement || event.target instanceof HTMLTextAreaElement) {
        return
      }

      const getBindingForKey = (event: KeyboardEvent): string | null => {
        // Build the current key combination
        const currentKeys: string[] = []

        // Add modifier keys first
        if (event.ctrlKey) currentKeys.push('Control')
        if (event.shiftKey) currentKeys.push('Shift')
        if (event.altKey) currentKeys.push('Alt')
        if (event.metaKey) currentKeys.push('Command')
        // Map special keys
        const specialKeys: { [key: string]: string } = {
          ArrowUp: 'ArrowUp',
          ArrowDown: 'ArrowDown',
          ArrowLeft: 'ArrowLeft',
          ArrowRight: 'ArrowRight',
          Enter: 'Enter',
          Escape: 'Escape',
          ' ': 'Space',
          Backspace: 'Backspace'
        }

        // Add the main key
        const mainKey = specialKeys[event.key] || event.key
        if (!currentKeys.includes(mainKey)) {
          currentKeys.push(mainKey)
        }

        // Check all bindings for a match
        const bindings = store.getAllBindings()
        for (const [action, binding] of Object.entries(bindings)) {
          // Ensure binding.currentKeys exists and is an array
          if (!Array.isArray(binding.currentKeys)) continue

          // Check each key combination in the binding
          for (const combo of binding.currentKeys) {
            // Ensure combo is an array
            if (!Array.isArray(combo)) continue

            // Check if the current keys match the combo
            if (
              currentKeys.length === combo.length &&
              currentKeys.every((key) => combo.includes(key))
            ) {
              return action
            }
          }
        }

        return null
      }

      const action = getBindingForKey(event)
      if (action && handlers[action]) {
        event.preventDefault()
        handlers[action]()
      }
    }

    window.addEventListener('keydown', handleKeyDown)
    return () => window.removeEventListener('keydown', handleKeyDown)
  }, [handlers, enabled])
}
