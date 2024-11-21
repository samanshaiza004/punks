/* eslint-disable @typescript-eslint/explicit-function-return-type */
// src/App.tsx

import { useState, useEffect } from 'react'
import AudioPlayer from './components/AudioPlayer'
import { useAudio } from './context/AudioContextProvider'
import SettingsModal from './components/settings/SettingsModal'
import { KeyBindingStore } from './keybinds/store'
import { useTheme } from './context/ThemeContext'
import { Button } from './components/Button'
import TabBar from './components/TabBar'
import { useTabs } from './context/TabContext'
import TabContainer from './components/TabContainer'
import { UpdateNotification } from './components/UpdateNotification'
import { Gear } from '@phosphor-icons/react/dist/ssr'
import { useDirectoryOperations } from './hooks/useDirectoryOperations'

interface DirectoryHistoryEntry {
  path: string[]
  timestamp: number
}

export function App() {
  const { isDarkMode } = useTheme()
  const [directoryPath, setDirectoryPath] = useState<string[]>([])
  const [directoryHistory, setDirectoryHistory] = useState<DirectoryHistoryEntry[]>([])
  const [currentHistoryIndex, setCurrentHistoryIndex] = useState<number>(-1)
  const [lastSelectedDirectory, setLastSelectedDirectory] = useState<string[]>([])
  const [currentAudio, _setCurrentAudio] = useState<string | null>(null)
  const [isSettingsOpen, setIsSettingsOpen] = useState(false)
  const { dispatch } = useTabs()
  const { playAudio } = useAudio()
  const { getLastDirectory } = useDirectoryOperations()

  const handleDirectoryChange = (newPath: string[]) => {
    if (JSON.stringify(newPath) === JSON.stringify(directoryPath)) {
      return
    }

    setDirectoryPath(newPath)

    const newEntry: DirectoryHistoryEntry = {
      path: newPath,
      timestamp: Date.now()
    }

    if (currentHistoryIndex < directoryHistory.length - 1) {
      setDirectoryHistory((prev: DirectoryHistoryEntry[]) => [
        ...prev.slice(0, currentHistoryIndex + 1),
        newEntry
      ])
    } else {
      setDirectoryHistory((prev: any) => [...prev, newEntry])
    }
    setCurrentHistoryIndex((prev: number) => prev + 1)
  }

  useEffect(() => {
    if (directoryPath.length > 0 && directoryHistory.length === 0) {
      setDirectoryHistory([{ path: directoryPath, timestamp: Date.now() }])
    }
  }, [directoryPath])

  useEffect(() => {
    const initializeApp = async () => {
      // Initialize key bindings
      const store = KeyBindingStore.getInstance()
      await store.initialize()

      // Get the last selected directory
      const lastDir = await getLastDirectory()
      if (lastDir) {
        setLastSelectedDirectory([lastDir])
        dispatch({
          type: 'ADD_TAB',
          payload: {
            directoryPath: [lastDir],
            searchQuery: '',
            searchResults: [],
            fileFilters: {
              all: true,
              audio: false,
              images: false,
              text: false,
              video: false,
              directories: false
            },
            title: 'Home'
          }
        })
      }
    }

    initializeApp()
  }, [dispatch, getLastDirectory])

  useEffect(() => {
    const keyDownHandler = (event: any) => {
      console.log('User pressed: ', event.key)

      if (event.key === 'Enter') {
        event.preventDefault()

        if (currentAudio) {
          playAudio(currentAudio)
        }
      }
    }

    document.addEventListener('keydown', keyDownHandler)

    return () => {
      document.removeEventListener('keydown', keyDownHandler)
    }
  }, [currentAudio, playAudio])

  return (
    <div
      className={`h-screen flex flex-col ${
        isDarkMode ? 'dark bg-gray-900 text-white' : 'bg-white text-gray-900'
      }`}
    >
      <div className="flex-shrink-0 ">
        <div className="flex items-center space-x-2 mb-2">
          <Button onClick={() => setIsSettingsOpen(true)} variant="secondary">
            <Gear size={20} weight="fill" />
          </Button>
        </div>
      </div>
        <TabBar lastSelectedDirectory={lastSelectedDirectory} />
      <div className="flex-shrink-0 mb-2">
        <AudioPlayer currentAudio={currentAudio} shouldReplay />
      </div>

      <div className="flex-grow overflow-hidden">
        <TabContainer directory={lastSelectedDirectory} />
      </div>
      <SettingsModal
        isOpen={isSettingsOpen}
        onClose={() => setIsSettingsOpen(false)}
        onDirectorySelected={(path: string[]) => handleDirectoryChange(path)}
        onSave={(_newBindings: any) => {
          // Handle the new bindings
          setIsSettingsOpen(false)
        }}
      />
      <UpdateNotification />
    </div>
  )
}
