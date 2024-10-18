// src/keybinds/hooks.ts
import { useEffect, useRef } from 'react'
import { KeyHandlerMap } from './types'
import { KeyBindingStore } from './store'

export function useKeyBindings(handlers: KeyHandlerMap, enabled = true) {
  const store = KeyBindingStore.getInstance()
  const handlersRef = useRef(handlers)

  useEffect(() => {
    handlersRef.current = handlers
  }, [handlers])

  useEffect(() => {
    if (!enabled) return

    const handleKeyDown = (event: KeyboardEvent) => {
      // Ignore key events when typing in input fields
      if (event.target instanceof HTMLInputElement || event.target instanceof HTMLTextAreaElement) {
        return
      }

      const currentKeys: string[] = []
      if (event.ctrlKey) currentKeys.push('Control')
      if (event.altKey) currentKeys.push('Alt')
      if (event.shiftKey) currentKeys.push('Shift')
      if (event.metaKey) currentKeys.push('Command')
      currentKeys.push(event.key)

      const keyCombo = currentKeys.join('+')

      Object.entries(store.getAllBindings()).forEach(([action, binding]) => {
        if (binding.currentKeys.some((combo) => combo.join('+') === keyCombo)) {
          handlersRef.current[action]?.()
        }
      })
    }

    window.addEventListener('keydown', handleKeyDown)
    return () => window.removeEventListener('keydown', handleKeyDown)
  }, [enabled, store])

  return null
}
