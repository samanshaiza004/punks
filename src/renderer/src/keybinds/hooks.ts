// src/keybinds/hooks.ts
import { useEffect, useCallback, useRef } from 'react'
import { KeyHandlerMap } from './types'
import { KeyBindingStore } from './store'

export function useKeyBindings(handlers: KeyHandlerMap, enabled = true) {
  const store = KeyBindingStore.getInstance()
  const handlersRef = useRef(handlers)

  // Update handlers ref when handlers change
  useEffect(() => {
    handlersRef.current = handlers
  }, [handlers])

  const handleKeyDown = useCallback(
    (event: KeyboardEvent) => {
      // Ignore key events when typing in input fields
      if (
        event.target instanceof HTMLInputElement ||
        event.target instanceof HTMLTextAreaElement ||
        !enabled
      ) {
        return
      }

      // Build the current key combination
      const currentKeys: string[] = []

      // Add modifier keys first (in consistent order)
      if (event.ctrlKey) currentKeys.push('Control')
      if (event.altKey) currentKeys.push('Alt')
      if (event.shiftKey) currentKeys.push('Shift')
      if (event.metaKey) currentKeys.push('Command')

      // Map special keys
      const specialKeys: { [key: string]: string } = {
        ' ': 'Space',
        ArrowUp: 'ArrowUp',
        ArrowDown: 'ArrowDown',
        ArrowLeft: 'ArrowLeft',
        ArrowRight: 'ArrowRight',
        Enter: 'Enter',
        Escape: 'Escape',
        Backspace: 'Backspace',
        Tab: 'Tab'
      }

      // Add the main key
      const mainKey =
        specialKeys[event.key] || event.key.length === 1 ? event.key.toUpperCase() : event.key
      if (!currentKeys.includes(mainKey)) {
        currentKeys.push(mainKey)
      }

      // Check all bindings for a match
      const bindings = store.getAllBindings()
      for (const [action, binding] of Object.entries(bindings)) {
        if (!Array.isArray(binding.currentKeys)) continue

        // Check each key combination in the binding
        for (const combo of binding.currentKeys) {
          if (
            currentKeys.length === combo.length &&
            currentKeys.every((key) => combo.includes(key))
          ) {
            event.preventDefault()
            if (handlersRef.current[action]) {
              handlersRef.current[action]()
            }
            return
          }
        }
      }
    },
    [enabled]
  )

  useEffect(() => {
    window.addEventListener('keydown', handleKeyDown)
    return () => window.removeEventListener('keydown', handleKeyDown)
  }, [handleKeyDown])
}
