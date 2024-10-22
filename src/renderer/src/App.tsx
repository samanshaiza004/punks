/* eslint-disable @typescript-eslint/explicit-function-return-type */
// src/App.tsx

import { useState, useEffect } from 'react'
import { DirectoryPicker } from './components/DirectoryPicker'
import DirectoryView from './components/DirectoryView'
import SearchBar from './components/SearchBar'
import FileGrid from './components/FileGrid'
import AudioPlayer from './components/AudioPlayer'
import { FileInfo } from './types/FileInfo'
import { useAudio } from './hooks/AudioContextProvider'
import MetadataDisplay from './components/MetadataDisplay'
import NavigationControls from './components/nav/NavigationControls'
import SettingsModal from './components/settings/SettingsModal'
import { KeyBindingStore } from './keybinds/store'
import { useTheme } from './context/ThemeContext'
import { Button } from './components/Button'

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
  const [volume, setVolume] = useState(0.8)

  const [searchQuery, setSearchQuery] = useState<string>('')
  const [searchResults, setSearchResults] = useState<FileInfo[]>([])

  const [files, setFiles] = useState<FileInfo[]>([])

  const [isSettingsOpen, setIsSettingsOpen] = useState(false)

  const [audioMetadata, setAudioMetadata] = useState<any>(null)
  const FILE_EXTENSIONS = {
    images: ['jpg', 'png'],
    text: ['txt', 'md'],
    audio: ['mp3', 'mp2', 'mp1', 'aif', 'aiff', 'aifc', 'wav', 'wave', 'bwf', 'flac', 'flc', 'ogg'],
    video: ['mp4', 'mov', 'avi']
  }

  const { playAudio } = useAudio()

  /** Fetches files from the current directory and sets the sorted result */
  const fetchFiles = () => {
    window.api
      .readDir(directoryPath)
      .then((result) => {
        const sortedFiles = sortFiles(result)
        setFiles(sortedFiles)
      })
      .catch((err) => window.api.sendMessage('App.tsx: ' + err))
  }

  /** Sorts files by directories first, then by name */
  const sortFiles = (files: FileInfo[]): FileInfo[] => {
    return files.sort((a, b) => {
      if (a.isDirectory && !b.isDirectory) return -1
      if (!a.isDirectory && b.isDirectory) return 1
      return a.name.localeCompare(b.name)
    })
  }

  const searchFiles = () => {
    if (searchQuery.length > 0) {
      window.api
        .search(directoryPath, searchQuery)
        .then((result: FileInfo[]) => {
          // Ensure result is always an array, even if empty
          const searchResults = Array.isArray(result) ? result : []
          setSearchResults(searchResults)
        })
        .catch((err: any) => {
          window.api.sendMessage('App.tsx: ' + err)
          setSearchResults([]) // Reset results on error
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
    if (searchQuery.length > 0) {
      setSearchQuery('')
      let newDirectory = directory
      window.api.sendMessage('App.tsx: newDirectory: ' + newDirectory)
      setDirectoryPath(newDirectory)
      setSearchResults([])
    } else {
      setDirectoryPath(directory)
    }
  }

  /* // Handle going back in history
  const handleGoBack = () => {
    if (currentHistoryIndex > 0) {
      const previousEntry = directoryHistory[currentHistoryIndex - 1]
      setDirectoryPath(previousEntry.path)
      setCurrentHistoryIndex((prev: number) => prev - 1)
    }
  }

  // Handle going forward in history
  const handleGoForward = () => {
    if (currentHistoryIndex < directoryHistory.length - 1) {
      const nextEntry = directoryHistory[currentHistoryIndex + 1]
      setDirectoryPath(nextEntry.path)
      setCurrentHistoryIndex((prev: number) => prev + 1)
    }
  } */

  useEffect(() => {
    if (directoryPath.length > 0 && directoryHistory.length === 0) {
      setDirectoryHistory([{ path: directoryPath, timestamp: Date.now() }])
      // setCurrentHistoryIndex(0)
    }
    console.log(currentHistoryIndex)
    fetchFiles()
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
        if (window.api.doesFileExist(audioPath)) {
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
        {/* <DirectoryPicker onDirectorySelected={(path: string[]) => handleDirectoryChange(path)} /> */}
        <div className="flex items-center space-x-4 mb-1">
          <Button onClick={() => setIsSettingsOpen(true)} variant="secondary" className="px-4 py-2">
            Settings
          </Button>
          <DirectoryView directoryPath={directoryPath} onDirectoryClick={handleDirectoryChange} />
        </div>

        <SearchBar
          searchQuery={searchQuery}
          setSearchQuery={setSearchQuery}
          onSearch={searchFiles}
          setSearchResults={setSearchResults}
        />
      </div>
      <div className="flex-shrink-0">
        <AudioPlayer
          currentAudio={currentAudio}
          volume={volume}
          setVolume={setVolume}
          shouldReplay
        />
      </div>

      <div className="flex flex-grow justify-between items-start gap-2.5 max-w-7xl mx-auto h-full overflow-auto mb-24">
        <FileGrid
          files={searchQuery.length > 0 ? searchResults : files}
          directoryPath={directoryPath}
          onDirectoryClick={handleDirectoryClick}
          onFileClick={handleFileClick}
          isSearching={searchQuery.length > 0}
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
