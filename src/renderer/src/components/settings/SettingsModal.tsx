import React, { useState, useEffect } from 'react'
import { KeyBindingMap } from '@renderer/types/keybinds'
import { defaultKeyBindings } from '@renderer/keybinds/defaults'
import { useTheme } from '@renderer/context/ThemeContext'
import { useToast } from '@renderer/context/ToastContext'
import TabButton from './TabButton'
import WindowSettings from './WindowSettings'
import KeybindSettings from './KeybindSettings'

interface SettingsModalProps {
  isOpen: boolean
  onClose: () => void
  onSave: (newBindings: KeyBindingMap) => void
  onDirectorySelected: (directory: string[]) => void
}

type SettingsTab = 'window' | 'keybinds'

const SettingsModal: React.FC<SettingsModalProps> = ({
  isOpen,
  onClose,
  onSave,
  onDirectorySelected
}) => {
  const [keyBindings, setKeyBindings] = useState<KeyBindingMap>(defaultKeyBindings)
  const [alwaysOnTop, setAlwaysOnTop] = useState(false)
  const [activeTab, setActiveTab] = useState<SettingsTab>('window')
  const { isDarkMode } = useTheme()
  const { showToast } = useToast()

  useEffect(() => {
    const loadSavedBindings = async (): Promise<void> => {
      try {
        const saved = await window.api.getKeyBindings()
        if (saved) {
          // Merge with defaults to ensure all actions are available
          const mergedBindings: KeyBindingMap = { ...defaultKeyBindings }
          Object.keys(saved).forEach((key) => {
            if (mergedBindings[key]) {
              mergedBindings[key] = {
                ...mergedBindings[key],
                currentKeys: saved[key].currentKeys
              }
            }
          })
          setKeyBindings(mergedBindings)
        } else {
          setKeyBindings(defaultKeyBindings)
        }
      } catch (error) {
        showToast('Failed to load key bindings: ' + error, 'error')
        setKeyBindings(defaultKeyBindings)
      }
    }

    const loadAlwaysOnTop = async (): Promise<void> => {
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

  const handleSave = async (): Promise<void> => {
    try {
      await window.api.saveKeyBindings(keyBindings)
      await window.api.setAlwaysOnTop(alwaysOnTop)
      onSave(keyBindings)
      onClose()
    } catch (error) {
      showToast('Failed to save settings: ' + error, 'error')
    }
  }

  if (!isOpen) return null

  return (
    <div className="fixed inset-0 bg-black/50 flex justify-center items-center z-50">
      <div
        className={`w-11/12 max-w-5xl max-h-[90vh] rounded-lg flex overflow-hidden ${
          isDarkMode ? 'bg-gray-800 text-gray-100' : 'bg-white text-gray-900'
        }`}
      >
        {/* Sidebar */}
        <div
          className={`w-64 flex-shrink-0 border-r ${
            isDarkMode ? 'bg-gray-900 border-gray-700' : 'bg-gray-50 border-gray-200'
          }`}
        >
          <div className="p-4">
            <h2 className="text-xl font-semibold">Settings</h2>
          </div>
          <nav>
            <TabButton
              isActive={activeTab === 'window'}
              onClick={() => setActiveTab('window')}
              icon="🖥️"
              label="Window"
            />
            <TabButton
              isActive={activeTab === 'keybinds'}
              onClick={() => setActiveTab('keybinds')}
              icon="⌨️"
              label="Keyboard"
            />
          </nav>
        </div>

        {/* Content */}
        <div className="flex-1 flex flex-col max-h-[90vh]">
          <div className="flex-1 overflow-y-auto p-6">
            {activeTab === 'window' ? (
              <WindowSettings
                alwaysOnTop={alwaysOnTop}
                setAlwaysOnTop={setAlwaysOnTop}
                onDirectorySelected={onDirectorySelected}
              />
            ) : (
              <KeybindSettings
                keyBindings={keyBindings}
                onUpdateBindings={setKeyBindings}
              />
            )}
          </div>

          <div
            className={`flex justify-end gap-2 p-4 border-t ${
              isDarkMode ? 'border-gray-700' : 'border-gray-200'
            }`}
          >
            <button
              onClick={onClose}
              className={`px-4 py-2 rounded transition-colors ${
                isDarkMode ? 'hover:bg-gray-700' : 'hover:bg-gray-100'
              }`}
            >
              Cancel
            </button>
            <button
              onClick={handleSave}
              className="px-4 py-2 bg-blue-500 text-white rounded hover:bg-blue-600 transition-colors"
            >
              Save Changes
            </button>
          </div>
        </div>
      </div>
    </div>
  )
}

export default SettingsModal
