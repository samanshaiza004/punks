import React, { useState, useEffect } from 'react'
import * as Dialog from '@radix-ui/react-dialog'
import * as Tabs from '@radix-ui/react-tabs'
import { KeyBindingMap } from '@renderer/types/keybinds'
import { defaultKeyBindings } from '@renderer/keybinds/defaults'
import { useTheme } from '@renderer/context/ThemeContext'
import { useToast } from '@renderer/context/ToastContext'
import { useUISettings, ItemSize } from '@renderer/context/UISettingsContext';
import { X, Gear, Keyboard } from '@phosphor-icons/react'
import WindowSettings from './WindowSettings'
import KeybindSettings from './KeybindSettings'

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
  const [alwaysOnTop, setAlwaysOnTop] = useState(false)
  const { isDarkMode } = useTheme()
  const { showToast } = useToast()
  const { settings, updateSettings } = useUISettings();

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
            w-[90vw] max-w-[800px] h-[90vh] max-h-[600px]
            rounded-lg flex overflow-hidden
            data-[state=open]:animate-contentShow
            focus:outline-none
            ${isDarkMode ? 'bg-gray-800 text-gray-100' : 'bg-white text-gray-900'}
          `}
        >
          <Tabs.Root
            defaultValue="window"
            orientation="vertical"
            className="flex w-full h-full"
          >
            <div
              className={`w-48 md:w-64 flex-shrink-0 border-r flex flex-col ${
                isDarkMode ? 'bg-gray-900 border-gray-700' : 'bg-gray-50 border-gray-200'
              }`}
            >
              <div className="p-4 flex justify-between items-center border-b border-inherit">
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
              <Tabs.List
                className="flex flex-col flex-1"
                aria-label="Settings sections"
              >
                <Tabs.Trigger
                  value="window"
                  className={`
                    flex items-center gap-2 px-4 py-2 text-left
                    focus:outline-none focus:bg-gray-700/10
                    data-[state=active]:bg-gray-700/20
                    hover:bg-gray-700/10
                    transition-colors
                  `}
                >
                  <Gear size={20} />
                  Window
                </Tabs.Trigger>
                <Tabs.Trigger
                  value="keybinds"
                  className={`
                    flex items-center gap-2 px-4 py-2 text-left
                    focus:outline-none focus:bg-gray-700/10
                    data-[state=active]:bg-gray-700/20
                    hover:bg-gray-700/10
                    transition-colors
                  `}
                >
                  <Keyboard size={20} />
                  Keyboard Shortcuts
                </Tabs.Trigger>
                <Tabs.Trigger
                  value="ui-size"
                  className={`
                    flex items-center gap-2 px-4 py-2 text-left
                    focus:outline-none focus:bg-gray-700/10
                    data-[state=active]:bg-gray-700/20
                    hover:bg-gray-700/10
                    transition-colors
                  `}
                >
                  UI Size
                </Tabs.Trigger>
              </Tabs.List>
            </div>
            <div className="flex-1 flex flex-col h-full">
              <div className="flex-1 overflow-y-auto p-6">
                <Tabs.Content value="window" className="outline-none h-full">
                  <WindowSettings
                    alwaysOnTop={alwaysOnTop}
                    setAlwaysOnTop={setAlwaysOnTop}
                    onDirectorySelected={onDirectorySelected}
                  />
                </Tabs.Content>
                <Tabs.Content value="keybinds" className="outline-none h-full">
                  <KeybindSettings
                    keyBindings={keyBindings}
                    onUpdateBindings={(newBindings) => setKeyBindings(newBindings)}
                  />
                </Tabs.Content>
                <Tabs.Content value="ui-size" className="outline-none h-full">
                  <div className="mb-6">
                    <h3 className="text-lg font-semibold mb-2">UI Size</h3>
                    <div className="space-y-2">
                      <label className="block">
                        <span className="text-sm font-medium">Item Size</span>
                        <select
                          value={settings.itemSize}
                          onChange={(e) => updateSettings({ itemSize: e.target.value as ItemSize })}
                          className="mt-1 block w-full rounded-md border-gray-300 dark:border-gray-600 dark:bg-gray-700 shadow-sm focus:border-blue-500 focus:ring-blue-500"
                        >
                          <option value="thin">Thin</option>
                          <option value="normal">Normal</option>
                          <option value="thick">Thick</option>
                        </select>
                      </label>
                    </div>
                  </div>
                </Tabs.Content>
              </div>

              <div
                className={`
                  p-4 border-t flex justify-end gap-2 flex-shrink-0
                  ${isDarkMode ? 'border-gray-700' : 'border-gray-200'}
                `}
              >
                <button
                  onClick={onClose}
                  className={`
                    px-4 py-2 rounded
                    ${
                      isDarkMode
                        ? 'hover:bg-gray-700 text-gray-300'
                        : 'hover:bg-gray-100 text-gray-600'
                    }
                  `}
                >
                  Cancel
                </button>
                <button
                  onClick={handleSave}
                  className="px-4 py-2 bg-blue-500 text-white rounded hover:bg-blue-600"
                >
                  Save Changes
                </button>
              </div>
            </div>
          </Tabs.Root>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  )
}

export default SettingsModal
