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

export function App() {
  const [directoryPath, setDirectoryPath] = useState<string[]>([])
  const [currentAudio, setCurrentAudio] = useState<string | null>(null)
  const [volume, setVolume] = useState(0.8)
  const [searchQuery, setSearchQuery] = useState<string>('')
  const [searchResults, setSearchResults] = useState<FileInfo[]>([])
  const [files, setFiles] = useState<FileInfo[]>([])
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

  /** Initiates a search for files and directories matching the query */
  const searchFiles = () => {
    if (searchQuery.length > 0) {
      window.api
        .search(directoryPath, searchQuery)
        .then((result: any) => setSearchResults(result))
        .catch((err: any) => window.api.sendMessage('App.tsx: ' + err))
        .finally(() => {})
    } else {
      setSearchResults([])
    }
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

  useEffect(() => {
    fetchFiles()
  }, [directoryPath])

  useEffect(() => {
    const initializeApp = async () => {
      const lastSelectedDirectory = await window.api.getLastSelectedDirectory()
      if (lastSelectedDirectory) {
        setDirectoryPath([lastSelectedDirectory])
      }
    }

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
    <div className="h-4/5 p-2.5 ">
      <DirectoryPicker onDirectorySelected={setDirectoryPath} />
      <DirectoryView directoryPath={directoryPath} onDirectoryClick={setDirectoryPath} />
      <SearchBar
        searchQuery={searchQuery}
        setSearchQuery={setSearchQuery}
        onSearch={searchFiles}
        setSearchResults={setSearchResults}
      />
      <div className="flex justify-between items-start gap-2.5 max-w-7xl mx-auto h-full">
        <FileGrid
          files={searchResults.length > 0 ? searchResults : files}
          directoryPath={directoryPath}
          onDirectoryClick={handleDirectoryClick}
          onFileClick={handleFileClick}
        />
        <div>
          <MetadataDisplay metadata={audioMetadata || null} />
        </div>
      </div>

      <AudioPlayer currentAudio={currentAudio} volume={volume} setVolume={setVolume} shouldReplay />
    </div>
  )
}
