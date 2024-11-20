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
}

const GRID_COLUMNS = 3
export const FileGrid: React.FC<FileGridProps> = ({
  directoryPath,
  onDirectoryClick,
  onFileClick,
  isSearching,
  searchResults,
  fileFilters
}) => {
  const { isDarkMode } = useTheme()
  const { showToast } = useToast()
  const [directories, setDirectories] = useState<DirectoryNode[]>([])
  const [files, setFiles] = useState<FileNode[]>([])
  const [isLoading, setIsLoading] = useState(true)
  const [isScanning, setIsScanning] = useState(false)
  const [selectedIndex, setSelectedIndex] = useState(0)
  const [scanProgress, setScanProgress] = useState<{ total: number; processed: number; percentComplete: number } | null>(null)
  
  const gridRef = useRef<HTMLDivElement>(null)

  // Filter files based on fileFilters
  const filteredFiles = useMemo(() => {
    if (!fileFilters) return files
    return files.filter(file => {
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

  // Combine directories and filtered files for display
  const displayItems = useMemo(() => {
    if (isSearching && searchResults) {
      return searchResults
    }
    return [...directories, ...filteredFiles]
  }, [directories, filteredFiles, isSearching, searchResults])

  // Load directory contents
  const loadDirectoryContents = async () => {
    try {
      setIsLoading(true)
      const contents = await window.api.getDirectoryContents(directoryPath)
      
      // Transform directories to match DirectoryNode type
      const transformedDirectories = contents.directories.map((dir, index) => ({
        ...dir,
        id: index, // Generate a unique ID based on index
        parent_path: directoryPath.join('/') || null, // Use current directory path as parent_path
        last_modified: dir.lastModified // Ensure last_modified is used
      }))
      
      setDirectories(transformedDirectories)
      setFiles(contents.files)
      setSelectedIndex(0) // Reset selection when directory changes
    } catch (error) {
      showToast('Error loading directory contents: ' + error, 'error')
    } finally {
      setIsLoading(false)
    }
  }

  // Set up event listeners for file changes
  useEffect(() => {
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
      setFiles(prev => [...prev, fileNode])
    }

    const handleFileRemoved = (path: string) => {
      setFiles(prev => prev.filter(f => f.path !== path))
    }

    const handleScanProgress = (progress: typeof scanProgress) => {
      setScanProgress(progress)
    }

    const handleScanComplete = () => {
      setIsScanning(false)
      setScanProgress(null)
      loadDirectoryContents()
      showToast('Directory scan complete!', 'success')
    }

    window.api.onFileAdded(handleFileAdded)
    window.api.onFileRemoved(handleFileRemoved)
    window.api.onScanProgress(handleScanProgress)
    window.api.onScanComplete(handleScanComplete)

    return () => {
      // Clean up listeners if needed
    }
  }, [])

  // Load contents when directory changes
  useEffect(() => {
    loadDirectoryContents()
  }, [directoryPath])

  // Handle keyboard navigation
  const handleNavigate = useCallback(
    (direction: 'up' | 'down' | 'left' | 'right') => {
      const maxIndex = displayItems.length - 1
      if (maxIndex < 0) return

      let newIndex = selectedIndex
      switch (direction) {
        case 'up':
          newIndex = Math.max(0, selectedIndex - GRID_COLUMNS)
          break
        case 'down':
          newIndex = Math.min(maxIndex, selectedIndex + GRID_COLUMNS)
          break
        case 'left':
          newIndex = Math.max(0, selectedIndex - 1)
          break
        case 'right':
          newIndex = Math.min(maxIndex, selectedIndex + 1)
          break
      }

      if (newIndex !== selectedIndex) {
        setSelectedIndex(newIndex)
        // Scroll into view
        const element = document.querySelector(`[data-index="${newIndex}"]`)
        element?.scrollIntoView({ block: 'nearest', behavior: 'smooth' })
      }
    },
    [selectedIndex, displayItems.length]
  )

  // Keyboard shortcut handlers
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
  const gridColumnsClass = useMemo(() => `grid-cols-${GRID_COLUMNS}`, [GRID_COLUMNS])

  // Loading state
  if (isLoading) {
    return (
      <div className="h-full w-full flex items-center justify-center">
        <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-blue-500" />
      </div>
    )
  }

  // Scanning state
  if (isScanning && scanProgress) {
    return <ScanProgress {...scanProgress} />
  }

  // Empty state
  if (displayItems.length === 0 && !isSearching) {
    return <EmptyStatePrompt onScanDirectory={handleScanDirectory} />
  }

  return (
    <div
      className={`
        grid
        gap-1
        p-2
        auto-rows-[35px]
        ${gridColumnsClass}
        h-full
        w-full
        overflow-auto
        ${isDarkMode ? 'bg-gray-900' : 'bg-white'}
      `}
      
      tabIndex={0}
      ref={gridRef}
    >
      {displayItems.map((item, index) => (
        <FileItem
          key={`${('type' in item ? item.type : 'file')}-${item.path}-${index}`}
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
