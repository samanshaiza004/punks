/* eslint-disable @typescript-eslint/explicit-function-return-type */
// src/renderer/src/keybinds/hooks.ts
import { useEffect, useRef } from 'react'
import { KeyHandlerMap } from '../types/keybinds'
import { KeyBindingStore } from './store'

// src/renderer/src/hooks/useKeyBindings.ts

export function useKeyBindings(handlers: KeyHandlerMap, enabled = true) {
  const store = KeyBindingStore.getInstance()
  const handlersRef = useRef(handlers)

  useEffect(() => {
    handlersRef.current = handlers
  }, [handlers])

  useEffect(() => {
    if (!enabled) return

    const handleKeyDown = (event: KeyboardEvent) => {
      // Debug logging
      console.log('Key event:', event.key)

      // Ignore key events when typing in input fields
      if (event.target instanceof HTMLInputElement || event.target instanceof HTMLTextAreaElement) {
        console.log('Ignoring key event in input field')
        return
      }

      const currentKeys: string[] = []

      // Add modifier keys in a consistent order
      if (event.ctrlKey) currentKeys.push('Control')
      if (event.altKey) currentKeys.push('Alt')
      if (event.shiftKey) currentKeys.push('Shift')
      if (event.metaKey) currentKeys.push('Command')

      // Special handling for Tab and other special keys
      let key = event.key
      if (key === 'Tab') key = 'Tab'
      else if (key === ' ') key = 'Space'
      else if (key.length === 1) key = key.toLowerCase()
      currentKeys.push(key)

      const keyCombo = currentKeys.join('+')
      
      let handled = false

      // Check for matching keybinds
      const bindings = store.getAllBindings()
      
      Object.entries(bindings).forEach(([action, binding]) => {
        const matchingCombo = binding.currentKeys.some((combo) => {
          if (!Array.isArray(combo)) return false
          const comboStr = combo.join('+')
          return comboStr === keyCombo
        })

        if (matchingCombo && handlersRef.current[action]) {
          event.preventDefault()
          event.stopPropagation()
          handled = true
          handlersRef.current[action]()
        }
      })

      if (handled) {
        console.log('Key event was handled')
        return false
      } else {
        console.log('No matching keybind found')
      }
    }

    window.addEventListener('keydown', handleKeyDown, true)
    return () => window.removeEventListener('keydown', handleKeyDown, true)
  }, [enabled, store])

  return null
}
