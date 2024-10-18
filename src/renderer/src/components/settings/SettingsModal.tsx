import React, { useState, useEffect } from 'react'
import { KeyBindingMap, KeyCombo } from '@renderer/keybinds/types'
import { defaultKeyBindings } from '@renderer/keybinds/defaults'

interface SettingsModalProps {
  isOpen: boolean
  onClose: () => void
  onSave: (newBindings: KeyBindingMap) => void
}

const SettingsModal: React.FC<SettingsModalProps> = ({ isOpen, onClose, onSave }) => {
  const [keyBindings, setKeyBindings] = useState<KeyBindingMap>(defaultKeyBindings)
  const [editingBinding, setEditingBinding] = useState<string | null>(null)
  const [tempKeys, setTempKeys] = useState<KeyCombo>([])

  useEffect(() => {
    const loadSavedBindings = async () => {
      try {
        const saved = await window.api.getKeyBindings()
        if (saved) {
          setKeyBindings(saved)
        }
      } catch (error) {
        console.error('Failed to load key bindings:', error)
      }
    }
    loadSavedBindings()
  }, [])

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (!editingBinding) return

    e.preventDefault()
    const key = e.key

    const modifiers = []
    if (e.ctrlKey) modifiers.push('Ctrl')
    if (e.altKey) modifiers.push('Alt')
    if (e.shiftKey) modifiers.push('Shift')

    const keyCombo = [...modifiers, key].join('+')
    setTempKeys((prev) => [...prev, keyCombo])
  }

  const startEditing = (bindingId: string) => {
    setEditingBinding(bindingId)
    setTempKeys([])
  }

  const saveBinding = () => {
    if (!editingBinding || tempKeys.length === 0) return

    setKeyBindings((prevBindings) => ({
      ...prevBindings,
      [editingBinding]: {
        ...prevBindings[editingBinding],
        currentKeys: [...prevBindings[editingBinding].currentKeys, tempKeys] // Adding the new key combination
      }
    }))

    setTempKeys([]) // Reset temporary keys
    setEditingBinding(null) // End editing mode
  }

  const resetToDefault = (bindingId: string) => {
    setKeyBindings((prev) => ({
      ...prev,
      [bindingId]: {
        ...prev[bindingId],
        currentKeys: [...prev[bindingId].defaultKeys]
      }
    }))
  }

  const handleSave = async () => {
    try {
      await window.api.saveKeyBindings(keyBindings)
      onSave(keyBindings)
      onClose()
    } catch (error) {
      console.error('Error saving key bindings:', error)
    }
  }

  if (!isOpen) return null

  return (
    <div className="fixed inset-0 bg-black bg-opacity-50 flex justify-center items-center z-50">
      <div className="bg-white rounded-lg w-11/12 max-w-4xl max-h-[90vh] overflow-y-auto">
        <div className="flex justify-between items-center p-4 border-b border-gray-200">
          <h2 className="text-xl font-semibold">Keyboard Shortcuts</h2>
          <button onClick={onClose} className="text-2xl hover:text-gray-700">
            ×
          </button>
        </div>

        <div className="p-4">
          <table className="w-full">
            <thead>
              <tr className="border-b border-gray-200">
                <th className="text-left p-3">Action</th>
                <th className="text-left p-3">Shortcut</th>
                <th className="text-left p-3">Description</th>
                <th className="text-left p-3">Actions</th>
              </tr>
            </thead>
            <tbody>
              {Object.values(keyBindings).map((binding) => (
                <tr key={binding.id} className="border-b border-gray-200">
                  <td className="p-3">{binding.action}</td>
                  <td className="p-3">
                    {editingBinding === binding.id ? (
                      <div className="flex gap-2">
                        <input
                          type="text"
                          value={tempKeys.join(' or ')}
                          placeholder="Press keys..."
                          onKeyDown={handleKeyDown}
                          readOnly
                          className="px-2 py-1 border border-gray-300 rounded w-40"
                        />
                        <button
                          onClick={saveBinding}
                          className="px-3 py-1 bg-blue-500 text-white rounded hover:bg-blue-600"
                        >
                          Save
                        </button>
                      </div>
                    ) : (
                      <button
                        onClick={() => startEditing(binding.id)}
                        className="px-3 py-1 bg-gray-100 border border-gray-300 rounded hover:bg-gray-200 min-w-[100px] text-left"
                      >
                        {binding.currentKeys
                          .map((combo) => combo.join('+')) // Join individual key combos with '+'
                          .join(' or ')}{' '}
                      </button>
                    )}
                  </td>
                  <td className="p-3">{binding.description}</td>
                  <td className="p-3">
                    <button
                      onClick={() => resetToDefault(binding.id)}
                      className="px-2 py-1 text-gray-600 hover:bg-gray-100 rounded"
                    >
                      Reset
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>

        <div className="flex justify-end gap-2 p-4 border-t border-gray-200">
          <button
            onClick={onClose}
            className="px-4 py-2 border border-gray-300 rounded hover:bg-gray-100"
          >
            Cancel
          </button>
          <button
            onClick={handleSave}
            className="px-4 py-2 bg-green-500 text-white rounded hover:bg-green-600"
          >
            Save Changes
          </button>
        </div>
      </div>
    </div>
  )
}

export default SettingsModal
