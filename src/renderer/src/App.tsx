/* eslint-disable @typescript-eslint/explicit-function-return-type */
// src/App.tsx

import { useState, useEffect } from 'react'

import DirectoryView from './components/DirectoryView'
import SearchBar from './components/SearchBar'
import FileGrid from './components/FileGrid'
import AudioPlayer from './components/AudioPlayer'
import { FileInfo } from './types/FileInfo'
import { useAudio } from './hooks/AudioContextProvider'

import SettingsModal from './components/settings/SettingsModal'
import { KeyBindingStore } from './keybinds/store'
import { useTheme } from './context/ThemeContext'
import { Button } from './components/Button'
import { FileFilter, FileFilterOptions } from './components/FileFilter'

interface DirectoryHistoryEntry {
  path: string[]
  timestamp: number
}

export function App() {
  const { isDarkMode } = useTheme()
  const [directoryPath, setDirectoryPath] = useState<string[]>([])

  const [directoryHistory, setDirectoryHistory] = useState<DirectoryHistoryEntry[]>([])
  const [currentHistoryIndex, setCurrentHistoryIndex] = useState<number>(-1)

  const [currentAudio, setCurrentAudio] = useState<string | null>(null)

  const [searchQuery, setSearchQuery] = useState<string>('')
  const [searchResults, setSearchResults] = useState<FileInfo[]>([])

  const [isSettingsOpen, setIsSettingsOpen] = useState(false)

  const [_audioMetadata, setAudioMetadata] = useState<any>(null)

  const [fileFilters, setFileFilters] = useState<FileFilterOptions>({
    all: true,
    audio: false,
    images: false,
    text: false,
    video: false,
    directories: false
  })

  const FILE_EXTENSIONS = {
    images: ['jpg', 'png'],
    text: ['txt', 'md'],
    audio: ['mp3', 'mp2', 'mp1', 'aif', 'aiff', 'aifc', 'wav', 'wave', 'bwf', 'flac', 'flc', 'ogg'],
    video: ['mp4', 'mov', 'avi']
  }

  const { playAudio } = useAudio()

  const searchFiles = () => {
    if (searchQuery.length > 0) {
      window.api
        .search(directoryPath, searchQuery)
        .then((result: FileInfo[]) => setSearchResults(result || []))
        .catch((err) => {
          window.api.sendMessage('App.tsx: ' + err)
          setSearchResults([])
        })
    } else {
      setSearchResults([])
    }
  }

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

  const handleDirectoryClick = (directory: string[]) => {
    setSearchQuery('')
    setDirectoryPath(directory)
    setSearchResults([])
  }

  useEffect(() => {
    if (directoryPath.length > 0 && directoryHistory.length === 0) {
      setDirectoryHistory([{ path: directoryPath, timestamp: Date.now() }])
    }
    console.log(currentHistoryIndex)
  }, [directoryPath])

  useEffect(() => {
    const initializeApp = async () => {
      const store = KeyBindingStore.getInstance()
      await store.initialize()

      const lastSelectedDirectory = await window.api.getLastSelectedDirectory()
      if (lastSelectedDirectory) {
        setDirectoryPath([lastSelectedDirectory])
      }
    }

    const store = KeyBindingStore.getInstance()
    store.initialize()

    initializeApp()

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
  }, [])

  const handleFileClick = async (file: FileInfo) => {
    setCurrentAudio('')
    const extension = file.name.split('.').pop()

    try {
      if (extension && FILE_EXTENSIONS.audio.includes(extension)) {
        let audioPath = ''
        if (searchQuery.length > 0) {
          console.log(file.location)
          audioPath = window.api.renderPath([file.location])
          window.api.sendMessage('' + audioPath)
        } else {
          audioPath = window.api.renderPath([...directoryPath, file.name])
          window.api.sendMessage('' + audioPath)
        }
        if (await window.api.doesFileExist(audioPath)) {
          const metadata = await window.api.getAudioMetadata(audioPath)
          setAudioMetadata(metadata)
          playAudio(`sample:///${audioPath}`)
        } else {
          throw new Error('File does not exist: ' + audioPath)
        }
      }
    } catch (err) {
      window.api.sendMessage('App.tsx: ' + err)
    }
  }

  return (
    <div
      className={`flex flex-col h-screen p-2.5 ${isDarkMode ? 'bg-gray-900 text-white' : 'bg-white text-black'}`}
    >
      <div className="flex-shrink-0 ">
        <div className="flex items-center space-x-2 mb-2">
          <Button onClick={() => setIsSettingsOpen(true)} variant="secondary" className="px-4 py-2">
            Settings
          </Button>
          <DirectoryView directoryPath={directoryPath} onDirectoryClick={handleDirectoryChange} />
        </div>

        <div className="flex items-center gap-4 mb-4">
          <SearchBar
            searchQuery={searchQuery}
            setSearchQuery={setSearchQuery}
            onSearch={searchFiles}
            setSearchResults={setSearchResults}
          />
          <FileFilter filters={fileFilters} onFilterChange={setFileFilters} />
        </div>
      </div>
      <div className="flex-shrink-0">
        <AudioPlayer currentAudio={currentAudio} shouldReplay />
      </div>

      <div className="flex flex-grow justify-between items-start gap-2.5 max-w-7xl mx-auto h-full overflow-auto mb-24">
        <FileGrid
          directoryPath={directoryPath}
          onDirectoryClick={handleDirectoryClick}
          onFileClick={handleFileClick}
          isSearching={searchQuery.length > 0}
          searchResults={searchResults}
          fileFilters={fileFilters}
        />
        {/* <div className="w-1/4 overflow-auto">
          <MetadataDisplay metadata={audioMetadata || null} />
        </div> */}
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
    </div>
  )
}
