import React, { useState } from 'react'
import { KeyBindingMap, KeyCombo } from '@renderer/types/keybinds'
import { Button } from '../Button'
import { useTheme } from '@renderer/context/ThemeContext'

interface KeybindSettingsProps {
  keyBindings: KeyBindingMap
  onUpdateBindings: (newBindings: KeyBindingMap) => void
}

const KeybindSettings: React.FC<KeybindSettingsProps> = ({ keyBindings, onUpdateBindings }) => {
  const { isDarkMode } = useTheme()
  const [editingBinding, setEditingBinding] = useState<string | null>(null)
  const [tempKeys, setTempKeys] = useState<string[]>([])

  const handleKeyDown = (e: React.KeyboardEvent): void => {
    if (!editingBinding) return

    e.preventDefault()
    const key = e.key
    const modifiers: string[] = []

    if (e.ctrlKey) modifiers.push('Control')
    if (e.altKey) modifiers.push('Alt')
    if (e.shiftKey) modifiers.push('Shift')
    if (e.metaKey) modifiers.push('Command')

    if (['Control', 'Alt', 'Shift', 'Meta'].includes(key)) return

    let finalKey = key
    if (key === ' ') finalKey = 'Space'
    else if (key.length === 1) finalKey = key.toLowerCase()

    const keyCombo = [...modifiers, finalKey]
    setTempKeys(keyCombo)
  }

  const startEditing = (bindingId: string): void => {
    setEditingBinding(bindingId)
    setTempKeys([])
  }

  const saveBinding = (): void => {
    if (!editingBinding || tempKeys.length === 0) return

    const newBindings = {
      ...keyBindings,
      [editingBinding]: {
        ...keyBindings[editingBinding],
        currentKeys: [tempKeys]
      }
    }

    onUpdateBindings(newBindings)
    setTempKeys([])
    setEditingBinding(null)
  }

  const resetToDefault = (bindingId: string): void => {
    const binding = keyBindings[bindingId]
    if (!binding) return

    const newBindings = {
      ...keyBindings,
      [bindingId]: {
        ...binding,
        currentKeys: [...binding.defaultKeys]
      }
    }

    onUpdateBindings(newBindings)
  }

  const formatKeyCombo = (combo: KeyCombo): string => {
    return combo.join(' + ')
  }

  const formatKeyBindingDisplay = (currentKeys: KeyCombo[]): string => {
    return currentKeys.map(formatKeyCombo).join(' or ')
  }

  return (
    <div className="space-y-4">
      <h3 className="text-lg font-semibold mb-4">Keyboard Shortcuts</h3>
      <div className="overflow-x-auto">
        <table className="w-full">
          <thead>
            <tr className={`border-b ${isDarkMode ? 'border-gray-700' : 'border-gray-200'}`}>
              <th className="text-left p-3">Action</th>
              <th className="text-left p-3">Shortcut</th>
              <th className="text-left p-3">Description</th>
              <th className="text-left p-3">Actions</th>
            </tr>
          </thead>
          <tbody>
            {Object.values(keyBindings).map((binding) => (
              <tr
                key={binding.id}
                className={`border-b ${isDarkMode ? 'border-gray-700' : 'border-gray-200'}`}
              >
                <td className="p-3">{binding.action}</td>
                <td className="p-3">
                  {editingBinding === binding.id ? (
                    <div className="flex gap-2">
                      <input
                        type="text"
                        value={tempKeys.join(' + ')}
                        placeholder="Press keys..."
                        onKeyDown={handleKeyDown}
                        readOnly
                        className={`px-2 py-1 border rounded w-40 ${
                          isDarkMode
                            ? 'bg-gray-700 border-gray-600'
                            : 'bg-white border-gray-300'
                        }`}
                      />
                      <Button onClick={saveBinding} variant="primary">
                        Save
                      </Button>
                    </div>
                  ) : (
                    <Button
                      onClick={() => startEditing(binding.id)}
                      variant="secondary"
                      className="min-w-[100px] text-left"
                    >
                      {formatKeyBindingDisplay(binding.currentKeys)}
                    </Button>
                  )}
                </td>
                <td className="p-3">{binding.description}</td>
                <td className="p-3">
                  <Button onClick={() => resetToDefault(binding.id)} variant="secondary">
                    Reset
                  </Button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  )
}

export default KeybindSettings
