/* eslint-disable @typescript-eslint/explicit-function-return-type */
import React, { useState, useEffect } from 'react'
import { KeyBindingMap, KeyCombo } from '@renderer/types/types'
import { defaultKeyBindings } from '@renderer/keybinds/defaults'
import { Button } from '../Button'
import { useTheme } from '@renderer/context/ThemeContext'
import { DirectoryPicker } from '../DirectoryPicker'

interface SettingsModalProps {
  isOpen: boolean
  onClose: () => void
  onSave: (newBindings: KeyBindingMap) => void
  onDirectorySelected: (directory: string[]) => void
}

const SettingsModal: React.FC<SettingsModalProps> = ({
  isOpen,
  onClose,
  onSave,
  onDirectorySelected
}) => {
  const [keyBindings, setKeyBindings] = useState<KeyBindingMap>(defaultKeyBindings)
  const [editingBinding, setEditingBinding] = useState<string | null>(null)
  const [tempKeys, setTempKeys] = useState<KeyCombo>([])
  const { isDarkMode, toggleDarkMode } = useTheme()
  useEffect(() => {
    const loadSavedBindings = async (): Promise<void> => {
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

    const modifiers: string[] = []
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

  const formatKeyCombo = (combo: KeyCombo | string): string => {
    if (typeof combo === 'string') return combo
    if (Array.isArray(combo)) return combo.join('+')
    return ''
  }

  const formatKeyBindingDisplay = (currentKeys: Array<KeyCombo | string>): string => {
    return currentKeys
      .map((combo) => formatKeyCombo(combo))
      .filter(Boolean)
      .join(' or ')
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
    <div className="fixed inset-0 bg-black/50 flex justify-center items-center z-50">
      <div
        className={`w-11/12 max-w-4xl max-h-[90vh] overflow-y-auto rounded-lg ${
          isDarkMode ? 'bg-gray-800 text-gray-100' : 'bg-white text-gray-900'
        }`}
      >
        <div
          className={`flex justify-between items-center p-4 border-b ${
            isDarkMode ? 'border-gray-700' : 'border-gray-200'
          }`}
        >
          <h2 className="text-xl font-semibold">Keyboard Shortcuts</h2>
          <button onClick={onClose} className={`text-2xl hover:opacity-75 transition-opacity`}>
            ×
          </button>
        </div>
        {/* Directory Picker */}
        <div className="mt-4 px-4">
          <h4 className="text-lg font-semibold">Select Directory</h4>
          <DirectoryPicker onDirectorySelected={onDirectorySelected} />
        </div>
        <div className="mt-4 px-4">
          <h3 className="text-lg font-semibold">Theme</h3>
          <Button variant="primary" onClick={toggleDarkMode}>
            {isDarkMode ? 'Switch to Light Mode' : 'Switch to Dark Mode'}
          </Button>
        </div>

        <div className="p-4">
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
                          value={tempKeys.join(' or ')}
                          placeholder="Press keys..."
                          onKeyDown={handleKeyDown}
                          readOnly
                          className={`px-2 py-1 border rounded w-40 ${
                            isDarkMode
                              ? 'bg-gray-700 border-gray-600 text-gray-100'
                              : 'bg-white border-gray-300 text-gray-900'
                          }`}
                        />
                        <button
                          onClick={saveBinding}
                          className="px-3 py-1 bg-blue-500 text-white rounded hover:bg-blue-600 transition-colors"
                        >
                          Save
                        </button>
                      </div>
                    ) : (
                      <button
                        onClick={() => startEditing(binding.id)}
                        className={`px-3 py-1 rounded min-w-[100px] text-left ${
                          isDarkMode
                            ? 'bg-gray-700 hover:bg-gray-600 border-gray-600'
                            : 'bg-gray-100 hover:bg-gray-200 border-gray-300'
                        } border transition-colors`}
                      >
                        {formatKeyBindingDisplay(binding.currentKeys)}
                      </button>
                    )}
                  </td>
                  <td className="p-3">{binding.description}</td>
                  <td className="p-3">
                    <button
                      onClick={() => resetToDefault(binding.id)}
                      className={`px-2 py-1 rounded ${
                        isDarkMode
                          ? 'text-gray-300 hover:bg-gray-700'
                          : 'text-gray-600 hover:bg-gray-100'
                      } transition-colors`}
                    >
                      Reset
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>

        <div
          className={`flex justify-end gap-2 p-4 border-t ${
            isDarkMode ? 'border-gray-700' : 'border-gray-200'
          }`}
        >
          <button
            onClick={onClose}
            className={`px-4 py-2 rounded border transition-colors ${
              isDarkMode ? 'border-gray-600 hover:bg-gray-700' : 'border-gray-300 hover:bg-gray-100'
            }`}
          >
            Cancel
          </button>
          <button
            onClick={handleSave}
            className="px-4 py-2 bg-green-500 text-white rounded hover:bg-green-600 transition-colors"
          >
            Save Changes
          </button>
        </div>
      </div>
    </div>
  )
}

export default SettingsModal
