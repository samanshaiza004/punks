import React, { useState, useEffect } from 'react'
import * as Dialog from '@radix-ui/react-dialog'
import { KeyBindingMap } from '@renderer/types/keybinds'
import { defaultKeyBindings } from '@renderer/keybinds/defaults'
import { useTheme } from '@renderer/context/ThemeContext'
import { useToast } from '@renderer/context/ToastContext'
import { X } from '@phosphor-icons/react'
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

  return (
    <Dialog.Root open={isOpen} onOpenChange={(open) => !open && onClose()}>
      <Dialog.Portal>
        <Dialog.Overlay
          className="fixed inset-0 bg-black/50 data-[state=open]:animate-overlayShow"
        />
        <Dialog.Content
          className={`
            fixed top-[50%] left-[50%] translate-x-[-50%] translate-y-[-50%]
            w-11/12 max-w-5xl max-h-[90vh]
            rounded-lg flex overflow-hidden
            data-[state=open]:animate-contentShow
            focus:outline-none
            ${isDarkMode ? 'bg-gray-800 text-gray-100' : 'bg-white text-gray-900'}
          `}
        >
          {/* Sidebar */}
          <div
            className={`w-64 flex-shrink-0 border-r ${
              isDarkMode ? 'bg-gray-900 border-gray-700' : 'bg-gray-50 border-gray-200'
            }`}
          >
            <div className="p-4 flex justify-between items-center">
              <Dialog.Title className="text-xl font-semibold">
                Settings
              </Dialog.Title>
              <Dialog.Close asChild>
                <button
                  className={`
                    rounded-full p-1.5
                    hover:bg-gray-700
                    focus:outline-none focus:ring-2 focus:ring-gray-400
                    transition-colors
                  `}
                  aria-label="Close"
                >
                  <X />
                </button>
              </Dialog.Close>
            </div>
            <nav>
              <TabButton
                isActive={activeTab === 'window'}
                onClick={() => setActiveTab('window')}
                icon={null}
                label="Window"
              />
              <TabButton
                isActive={activeTab === 'keybinds'}
                onClick={() => setActiveTab('keybinds')}
                icon={null}
                label="Keyboard Shortcuts"
              />
            </nav>
          </div>

          {/* Content */}
          <div className="flex-1 overflow-auto">
            <div className="p-6">
              {activeTab === 'window' ? (
                <WindowSettings
                  alwaysOnTop={alwaysOnTop}
                  setAlwaysOnTop={setAlwaysOnTop}
                  onDirectorySelected={onDirectorySelected}
                />
              ) : (
                <KeybindSettings
                    keyBindings={keyBindings}
                    onUpdateBindings={(newBindings) => setKeyBindings(newBindings)}
                  />
              )}
            </div>

            {/* Footer */}
            <div
              className={`
                sticky bottom-0 left-0 right-0
                p-4 mt-auto
                border-t
                flex justify-end gap-2
                ${isDarkMode ? 'bg-gray-800 border-gray-700' : 'bg-white border-gray-200'}
              `}
            >
              <Dialog.Close asChild>
                <button
                  className={`
                    px-4 py-2 rounded
                    ${
                      isDarkMode
                        ? 'hover:bg-gray-700 text-gray-300'
                        : 'hover:bg-gray-100 text-gray-700'
                    }
                  `}
                >
                  Cancel
                </button>
              </Dialog.Close>
              <button
                onClick={handleSave}
                className={`
                  px-4 py-2 rounded
                  bg-blue-500 text-white
                  hover:bg-blue-600
                  transition-colors
                `}
              >
                Save Changes
              </button>
            </div>
          </div>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  )
}

export default SettingsModal
