import React, { useEffect, useState, useRef, useMemo, useCallback } from 'react'
import { useTheme } from '@renderer/context/ThemeContext'
import { FileNode, DirectoryNode, AudioFile } from '../../../../types'
import { FileItem } from '../FileItem'
import EmptyStatePrompt from '../EmptyStatePrompt'
import ScanProgress from '../ScanProgress'
import { useToast } from '@renderer/context/ToastContext'
import { KeyHandlerMap } from '../../types/keybinds'
import { useKeyBindings } from '@renderer/keybinds/hooks'
import path from 'path'
import { FileFilterOptions } from '../FileFilters'

interface FileGridProps {
  directoryPath: string[]
  onDirectoryClick: (path: string[]) => void
  onFileClick: (file: FileNode) => void
  isSearching?: boolean
  searchResults?: FileNode[]
  fileFilters?: FileFilterOptions
  autoPlay?: boolean
  playAudio?: (path: string) => void
}

const isAudioFile = (filePath: string): boolean => {
  const audioExtensions = ['mp3', 'wav', 'ogg', 'aac', 'm4a']
  const fileExtension = path.extname(filePath).toLowerCase().slice(1)
  return audioExtensions.includes(fileExtension)
}

const COLUMN_WIDTHS = [
  { maxWidth: 600, columns: 1 },
  { maxWidth: 900, columns: 2 },
  { maxWidth: 1200, columns: 3 },
  { maxWidth: Infinity, columns: 4 }
]

export const FileGrid: React.FC<FileGridProps> = ({
  directoryPath,
  onDirectoryClick,
  onFileClick,
  isSearching,
  searchResults,
  fileFilters,
  autoPlay = false,
  playAudio
}) => {
  const { isDarkMode } = useTheme()
  const { showToast } = useToast()
  const [directories, setDirectories] = useState<DirectoryNode[]>([])
  const [files, setFiles] = useState<FileNode[]>([])
  const [isLoading, setIsLoading] = useState(true)
  const [isScanning, setIsScanning] = useState(false)
  const [selectedIndex, setSelectedIndex] = useState(0)
  const [scanProgress, setScanProgress] = useState<{
    total: number
    processed: number
    percentComplete: number
  } | null>(null)

  const [gridColumns, setGridColumns] = useState(3)
  const gridRef = useRef<HTMLDivElement>(null)

  const filteredFiles = useMemo(() => {
    if (!fileFilters) return files
    return files.filter((file) => {
      const extension = file.name.split('.').pop()?.toLowerCase() || ''
      return Object.entries(fileFilters).some(([type, isEnabled]) => {
        if (!isEnabled) return false
        switch (type) {
          case 'audio':
            return ['mp3', 'wav', 'flac', 'ogg'].includes(extension)
          case 'image':
            return ['jpg', 'jpeg', 'png', 'gif'].includes(extension)
          case 'video':
            return ['mp4', 'mov', 'avi'].includes(extension)
          default:
            return true
        }
      })
    })
  }, [files, fileFilters])

  const ensureSelectedItemInView = useCallback((index: number) => {
    requestAnimationFrame(() => {
      const container = gridRef.current
      if (!container) return

      const element = container.querySelector(`[data-index="${index}"]`) as HTMLElement
      if (!element) return

      const containerRect = container.getBoundingClientRect()
      const elementRect = element.getBoundingClientRect()

      // Calculate scroll positions
      const scrollTop = container.scrollTop
      const containerTop = containerRect.top
      const containerBottom = containerRect.bottom
      const elementTop = elementRect.top
      const elementBottom = elementRect.bottom

      // Determine if the element is fully visible
      const isAboveContainer = elementTop < containerTop
      const isBelowContainer = elementBottom > containerBottom

      if (isAboveContainer) {
        // Scroll up to show the element
        container.scrollTop = scrollTop + (elementTop - containerTop)
      } else if (isBelowContainer) {
        // Scroll down to show the element
        container.scrollTop = scrollTop + (elementBottom - containerBottom)
      }
    })
  }, [])

  const displayItems = useMemo(() => {
    if (isSearching && searchResults) {
      return searchResults
    }
    return [...directories, ...filteredFiles]
  }, [directories, filteredFiles, isSearching, searchResults])

  const loadFilesRecursively = useCallback(async (dirPath: string): Promise<FileNode[]> => {
    try {
      const contents = await window.api.getDirectoryContents([dirPath])
      let allFiles: FileNode[] = contents.files

      // Recursively get files from subdirectories
      for (const dir of contents.directories) {
        const subFiles = await loadFilesRecursively(dir.path)
        allFiles = [...allFiles, ...subFiles]
      }

      return allFiles
    } catch (error) {
      console.error('Error loading files recursively:', error)
      return []
    }
  }, [])

  useEffect(() => {
    let mounted = true
    const loadFiles = async () => {
      if (!directoryPath.length || !directoryPath[0]) return

      setIsLoading(true)
      try {
        const allFiles = await loadFilesRecursively(directoryPath[0])
        if (mounted) {
          setFiles(allFiles)
        }
      } catch (error) {
        console.error('Error loading files:', error)
      }
      if (mounted) {
        setIsLoading(false)
      }
    }

    loadFiles()
    return () => {
      mounted = false
    }
  }, [directoryPath, loadFilesRecursively])

  const handleFileAdded = (file: AudioFile) => {
    const fileNode: FileNode = {
      id: file.id,
      path: file.path,
      name: path.basename(file.path),
      type: 'file' as const,
      directory_path: path.dirname(file.path),
      hash: file.hash,
      tags: file.tags,
      last_modified: file.last_modified
    }
    setFiles((prev) => [...prev, fileNode])
  }

  const handleFileRemoved = (path: string) => {
    setFiles((prev) => prev.filter((f) => f.path !== path))
  }

  const handleScanProgress = (progress: typeof scanProgress) => {
    setScanProgress(progress)
  }

  const handleScanComplete = () => {
    setIsScanning(false)
    setScanProgress(null)
    setIsLoading(true)
    const loadFiles = async () => {
      try {
        if (directoryPath.length > 0) {
          const allFiles = await loadFilesRecursively(directoryPath[0])
          setFiles(allFiles)
        }
      } catch (error) {
        console.error('Error loading files:', error)
      }
      setIsLoading(false)
    }
    loadFiles()
    showToast('Directory scan complete!', 'success')
  }

  const calculateColumns = () => {
    if (!gridRef.current) return

    const gridWidth = gridRef.current.clientWidth
    const columnConfig =
      COLUMN_WIDTHS.find((config) => gridWidth <= config.maxWidth) ||
      COLUMN_WIDTHS[COLUMN_WIDTHS.length - 1]

    setGridColumns(columnConfig.columns)
  }

  useEffect(() => {
    window.api.onFileAdded(handleFileAdded)
    window.api.onFileRemoved(handleFileRemoved)
    window.api.onScanProgress(handleScanProgress)
    window.api.onScanComplete(handleScanComplete)

    calculateColumns()

    window.addEventListener('resize', calculateColumns)

    return () => {
      window.removeEventListener('resize', calculateColumns)
    }
  }, [])

  const handleNavigate = useCallback(
    (direction: 'up' | 'down' | 'left' | 'right') => {
      const maxIndex = displayItems.length - 1
      if (maxIndex < 0) return

      let newIndex = selectedIndex
      switch (direction) {
        case 'up':
          newIndex = selectedIndex >= gridColumns ? selectedIndex - gridColumns : selectedIndex
          break
        case 'down':
          newIndex = selectedIndex + gridColumns <= maxIndex ? selectedIndex + gridColumns : selectedIndex
          break
        case 'left':
          newIndex = selectedIndex > 0 ? selectedIndex - 1 : selectedIndex
          break
        case 'right':
          newIndex = selectedIndex < maxIndex ? selectedIndex + 1 : selectedIndex
          break
      }

      if (newIndex !== selectedIndex) {
        setSelectedIndex(newIndex)
        
        // Auto-play the selected audio file if it's an audio file and auto-play is enabled
        const selectedItem = displayItems[newIndex]
        if (
          selectedItem && 
          'type' in selectedItem && 
          selectedItem.type === 'file' && 
          autoPlay && 
          isAudioFile(selectedItem.path) && 
          playAudio
        ) {
          onFileClick(selectedItem)
        }

        ensureSelectedItemInView(newIndex)
      }
    },
    [selectedIndex, displayItems.length, gridColumns, autoPlay, playAudio]
  )

  useEffect(() => {
    if (displayItems.length > 0 && selectedIndex === 0) {
      ensureSelectedItemInView(0)
    }
  }, [displayItems, selectedIndex, ensureSelectedItemInView])

  const handlers = useMemo<KeyHandlerMap>(
    () => ({
      NAVIGATE_UP: () => handleNavigate('up'),
      NAVIGATE_DOWN: () => handleNavigate('down'),
      NAVIGATE_LEFT: () => handleNavigate('left'),
      NAVIGATE_RIGHT: () => handleNavigate('right'),
      SELECT_ITEM: () => {
        const item = displayItems[selectedIndex]
        if (!item) return

        if ('type' in item && item.type === 'directory') {
          onDirectoryClick([...directoryPath, item.name])
        } else {
          onFileClick(item as FileNode)
        }
      }
    }),
    [handleNavigate, displayItems, selectedIndex, directoryPath, onDirectoryClick, onFileClick]
  )

  // Enable keyboard shortcuts
  useKeyBindings(handlers, true)

  // Handle directory scanning
  const handleScanDirectory = async () => {
    try {
      const directory = await window.api.openDirectoryPicker()
      if (!directory) return

      setIsScanning(true)
      setScanProgress({ total: 0, processed: 0, percentComplete: 0 }) // Initialize progress
      await window.api.scanDirectory(directory)
    } catch (error) {
      showToast('Error initiating directory scan: ' + error, 'error')
      setIsScanning(false)
      setScanProgress(null)
    }
  }

  // Get grid columns class dynamically
  const gridColumnsClass = useMemo(() => `grid-cols-${gridColumns}`, [gridColumns])

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

  if (displayItems.length === 0 && !isSearching) {
    return <EmptyStatePrompt onScanDirectory={handleScanDirectory} />
  }

  return (
    <div
      ref={gridRef}
      className={`
        grid
        p-2
        auto-rows-[30px]
        ${gridColumnsClass}
        h-full
        w-full
        overflow-y-auto
      `}
      tabIndex={0}
    >
      {displayItems.map((item, index) => (
        <FileItem
          key={`${'type' in item ? item.type : 'file'}-${item.path}-${index}`}
          data-index={index}
          fileName={item.name}
          isDirectory={'type' in item && item.type === 'directory'}
          location={item.path}
          isSelected={index === selectedIndex}
          isDarkMode={isDarkMode}
          onClick={() => {
            setSelectedIndex(index)
            if ('type' in item && item.type === 'directory') {
              onDirectoryClick([...directoryPath, item.name])
            } else {
              onFileClick(item as FileNode)
            }
          }}
        />
      ))}
    </div>
  )
}

export default FileGrid
