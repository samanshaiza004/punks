import React, { useEffect, useState, useRef } from 'react'
import { useTheme } from '@renderer/context/ThemeContext'
import { FileInfo, Directory } from '@renderer/types'
import { FileItem } from '../FileItem'
import EmptyStatePrompt from '../EmptyStatePrompt'
import ScanProgress from '../ScanProgress'
import { useToast } from '@renderer/context/ToastContext'

interface FileGridProps {
  directoryPath: string[]
  onDirectoryClick: (path: string[]) => void
  onFileClick: (file: FileInfo, index: number) => void
  selectedIndex?: number
  isSearching?: boolean
  searchResults?: FileInfo[]
  fileFilters?: Record<string, boolean>
}

const FileGrid: React.FC<FileGridProps> = ({
  directoryPath,
  onDirectoryClick,
  onFileClick,
  selectedIndex,
  isSearching,
  searchResults,
  fileFilters
}) => {
  const { isDarkMode } = useTheme()
  const { showToast } = useToast()
  const [directories, setDirectories] = useState<Directory[]>([])
  const [files, setFiles] = useState<FileInfo[]>([])
  const [isLoading, setIsLoading] = useState(true)
  const [isScanning, setIsScanning] = useState(false)
  const [scanProgress, setScanProgress] = useState<{
    total: number
    processed: number
    percentComplete: number
  } | null>(null)

  const gridRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    loadDirectoryContents()
    setupEventListeners()
  }, [directoryPath])

  const loadDirectoryContents = async () => {
    try {
      setIsLoading(true)
      const contents = await window.api.getDirectoryContents(directoryPath)
      setDirectories(contents.directories)
      setFiles(contents.files)
    } catch (error) {
      showToast('Error loading directory contents: ' + error, 'error')
    } finally {
      setIsLoading(false)
    }
  }

  const setupEventListeners = () => {
    window.api.onFileAdded((file) => {
      setFiles((prev) => [...prev, file])
    })

    window.api.onFileChanged((file) => {
      setFiles((prev) =>
        prev.map((f) => (f.path === file.path ? file : f))
      )
    })

    window.api.onFileRemoved((path) => {
      setFiles((prev) => prev.filter((f) => f.path !== path))
    })

    window.api.onScanProgress((progress) => {
      setScanProgress(progress)
    })

    window.api.onScanComplete(() => {
      setIsScanning(false)
      setScanProgress(null)
      loadDirectoryContents()
      showToast('Directory scan complete!', 'success')
    })

    window.api.onScanError((error) => {
      setIsScanning(false)
      setScanProgress(null)
      showToast('Error scanning directory: ' + error, 'error')
    })
  }

  const handleScanDirectory = async () => {
    try {
      const directory = await window.api.openDirectoryPicker()
      if (!directory) return

      setIsScanning(true)
      await window.api.scanDirectory(directory)
    } catch (error) {
      showToast('Error initiating directory scan: ' + error, 'error')
      setIsScanning(false)
    }
  }

  if (isLoading) {
    return (
      <div className="h-full w-full flex items-center justify-center">
        <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-blue-500" />
      </div>
    )
  }

  if (isScanning && scanProgress) {
    return <ScanProgress {...scanProgress} />
  }

  if (directories.length === 0 && files.length === 0 && !isSearching) {
    return <EmptyStatePrompt onScanDirectory={handleScanDirectory} />
  }

  const renderableItems = [
    ...directories.map((dir, index) => ({
      ...dir,
      isDirectory: true,
      index
    })),
    ...files.map((file, index) => ({
      ...file,
      isDirectory: false,
      index: index + directories.length
    }))
  ]

  return (
    <div
      className={`
        grid
        gap-1
        p-2
        auto-rows-[35px]
        grid-cols-1
        sm:grid-cols-2
        md:grid-cols-3
        lg:grid-cols-4
        h-full
        w-full
        overflow-auto
        ${isDarkMode ? 'bg-gray-900' : 'bg-white'}
      `}
      role="grid"
      tabIndex={0}
      ref={gridRef}
    >
      {renderableItems.map((item) => (
        <FileItem
          key={item.path}
          data-index={item.index}
          fileName={item.name}
          isDirectory={item.isDirectory}
          location={item.path}
          isSelected={item.index === selectedIndex}
          isDarkMode={isDarkMode}
          onClick={() => {
            if (item.isDirectory) {
              onDirectoryClick([...directoryPath, item.name])
            } else {
              onFileClick(item as FileInfo, item.index)
            }
          }}
        />
      ))}
    </div>
  )
}

export default FileGrid
