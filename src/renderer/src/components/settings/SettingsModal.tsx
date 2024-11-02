import React, { useState, useEffect } from 'react'
import { KeyBindingMap } from '@renderer/types/types'
import { defaultKeyBindings } from '@renderer/keybinds/defaults'
import { Button } from '../Button'
import { useTheme } from '@renderer/context/ThemeContext'
import { DirectoryPicker } from '../DirectoryPicker'
import { useToast } from '@renderer/context/ToastContext'

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
  const [tempKeys, setTempKeys] = useState<string[]>([])
  const [alwaysOnTop, setAlwaysOnTop] = useState(false)
  const { isDarkMode, toggleDarkMode } = useTheme()
  const { showToast } = useToast()
  useEffect(() => {
    const loadSavedBindings = async () => {
      try {
        const saved = await window.api.getKeyBindings()
        if (saved) {
          const mergedBindings = {
            ...saved
          }
          setKeyBindings(mergedBindings)
        }
      } catch (error) {
        showToast('Failed to load key bindings: ' + error, 'error')
        setKeyBindings(defaultKeyBindings)
      }
    }

    const loadAlwaysOnTop = async () => {
      try {
        const value = await window.api.getAlwaysOnTop()
        setAlwaysOnTop(value)
      } catch (error) {
        showToast('an error occured: ' + error, 'error')
      }
    }

    loadAlwaysOnTop()
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

    if (['Control', 'Alt', 'Shift'].includes(key)) return

    const keyCombo = [...modifiers, key]
    setTempKeys(keyCombo)
  }

  const startEditing = (bindingId: string) => {
    setEditingBinding(bindingId)
    setTempKeys([])
  }

  const saveBinding = () => {
    if (!editingBinding || tempKeys.length === 0) return

    setKeyBindings((prevBindings) => {
      const binding = prevBindings[editingBinding]
      if (!binding) return prevBindings

      return {
        ...prevBindings,
        [editingBinding]: {
          ...binding,
          currentKeys: [tempKeys]
        }
      }
    })

    setTempKeys([])
    setEditingBinding(null)
  }

  const formatKeyCombo = (combo: string[]): string => {
    if (typeof combo === 'string') return combo
    if (Array.isArray(combo)) return combo.join('+')
    return ''
  }

  const formatKeyBindingDisplay = (currentKeys: string[][]): string => {
    return currentKeys.map((combo) => formatKeyCombo(combo)).join(' or ')
  }

  const resetToDefault = (bindingId: string) => {
    setKeyBindings((prev) => {
      const binding = prev[bindingId]
      if (!binding) return prev

      return {
        ...prev,
        [bindingId]: {
          ...binding,
          currentKeys: [...binding.defaultKeys]
        }
      }
    })
  }

  const handleSave = async () => {
    try {
      await window.api.saveKeyBindings(keyBindings)
      await window.api.setAlwaysOnTop(alwaysOnTop)
      onSave(keyBindings)
      onClose()
    } catch (error) {
      showToast('Failed to save key bindings: ' + error, 'error')
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
        <div className="mt-4 px-4">
          <h4 className="text-lg font-semibold">Window Settings</h4>
          <label>
            <input
              type="checkbox"
              checked={alwaysOnTop}
              onChange={(e) => setAlwaysOnTop(e.target.checked)}
            />{' '}
            Always on Top
          </label>
        </div>
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
                              : 'bg-white border-gray-300 text-gray-90'
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
