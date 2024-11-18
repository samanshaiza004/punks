import React, { useEffect, useState, useRef, useCallback, useMemo } from 'react'
import { FileItem } from '../FileItem'
import { FileInfo } from '@renderer/types/FileInfo'
import { useTheme } from '@renderer/context/ThemeContext'
import { useBatchLoading } from '@renderer/hooks/useBatchLoading'
import { useKeyBindings } from '@renderer/keybinds/hooks'
import { KeyHandlerMap } from '@renderer/types/keybinds'
import { filterFiles } from '@renderer/utils/fileFilters'
import { FileFilterOptions } from '../FileFilters'

interface FileGridProps {
  directoryPath: string[]
  onDirectoryClick: (path: string[]) => void
  onFileClick: (file: FileInfo) => void
  isSearching?: boolean
  searchResults?: FileInfo[]
  fileFilters: FileFilterOptions
}

const FileGrid: React.FC<FileGridProps> = ({
  directoryPath,
  onDirectoryClick,
  onFileClick,
  isSearching = false,
  searchResults = [],
  fileFilters
}) => {
  const { files, isLoading, hasMore, loadMoreFiles, totalFiles } = useBatchLoading(directoryPath)
  const [selectedIndex, setSelectedIndex] = useState(0)
  const { isDarkMode } = useTheme()
  const observerRef = useRef<IntersectionObserver | null>(null)
  const loadingTriggerRef = useRef<HTMLDivElement>(null)
  const gridRef = useRef<HTMLDivElement>(null)

  const filteredFiles = useMemo(() => {
    const sourceFiles = isSearching ? searchResults : files
    return filterFiles(sourceFiles, fileFilters)
  }, [isSearching, searchResults, files, fileFilters])

  useEffect(() => {
    const options = {
      root: null,
      rootMargin: '100px',
      threshold: 0.1
    }

    observerRef.current = new IntersectionObserver(([entry]) => {
      if (entry.isIntersecting && hasMore && !isLoading) {
        loadMoreFiles()
      }
    }, options)

    if (loadingTriggerRef.current) {
      observerRef.current.observe(loadingTriggerRef.current)
    }

    return (): void => observerRef.current?.disconnect()
  }, [hasMore, isLoading, isSearching, loadMoreFiles])

  const displayedFiles = isSearching ? searchResults : files

  const navigateFiles = useCallback(
    (direction: 'up' | 'down' | 'left' | 'right') => {
      setSelectedIndex((prevIndex) => {
        let newIndex = prevIndex
        const maxIndex = filteredFiles.length - 1
        const currentRow = Math.floor(prevIndex / 4)
        const currentCol = prevIndex % 4
        const totalRows = Math.ceil(filteredFiles.length / 4)

        switch (direction) {
          case 'up': {
            const newRow = currentRow - 1
            if (newRow >= 0) {
              newIndex = newRow * 4 + currentCol
              if (newIndex > maxIndex) {
                newIndex = maxIndex
              }
            }
            break
          }
          case 'down': {
            const newRow = currentRow + 1
            if (newRow < totalRows) {
              newIndex = Math.min(newRow * 4 + currentCol, maxIndex)
            }
            break
          }
          case 'left': {
            if (currentCol > 0) {
              newIndex = prevIndex - 1
            }
            break
          }
          case 'right': {
            if (currentCol < 4 - 1 && prevIndex < maxIndex) {
              newIndex = prevIndex + 1
            }
            break
          }
        }

        newIndex = Math.max(0, Math.min(newIndex, maxIndex))

        const selectedElement = document.querySelector(`[data-index="${newIndex}"]`)
        if (selectedElement) {
          selectedElement.scrollIntoView({ block: 'nearest', behavior: 'smooth' })
        }

        return newIndex
      })
    },
    [filteredFiles.length]
  )

  const handlers: KeyHandlerMap = useMemo(
    () => ({
      NAVIGATE_UP: () => navigateFiles('up'),
      NAVIGATE_DOWN: () => navigateFiles('down'),
      NAVIGATE_LEFT: () => navigateFiles('left'),
      NAVIGATE_RIGHT: () => navigateFiles('right'),
      SELECT_ITEM: (): void => {
        const selectedFile = filteredFiles[selectedIndex]
        if (selectedFile) {
          if (selectedFile.isDirectory) {
            onDirectoryClick([...directoryPath, selectedFile.name])
          } else {
            onFileClick(selectedFile)
          }
        }
      }
    }),
    [navigateFiles, filteredFiles, selectedIndex, directoryPath, onDirectoryClick, onFileClick]
  )

  useKeyBindings(handlers)

  useEffect(() => {
    setSelectedIndex(0)
  }, [directoryPath])

  if (displayedFiles.length === 0 && !isLoading) {
    return (
      <div
        className={`col-span-full flex flex-col items-center justify-center p-8 rounded-lg
        ${isDarkMode ? 'bg-gray-700 text-gray-200' : 'bg-gray-50 text-gray-600'}`}
      >
        {isSearching ? (
          <>
            <p className="text-xl font-semibold mb-2">No search results found</p>
            <p className="text-sm">Try adjusting your search terms</p>
          </>
        ) : (
          <>
            <p className="text-xl font-semibold mb-2">This folder is empty</p>
            <p className="text-sm">Add some files to get started</p>
          </>
        )}
      </div>
    )
  }

  const handleFileClick = (file: FileInfo, index: number) => {
    setSelectedIndex(index)
    if (file.isDirectory) {
      onDirectoryClick([...directoryPath, file.name])
    } else {
      onFileClick(file)
    }
  }

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
      {filteredFiles.map((file, index) => (
        <FileItem
          key={file.name + index}
          data-index={index}
          fileName={file.name}
          isDirectory={file.isDirectory}
          location={window.api.renderPath([...directoryPath, file.name])}
          isSelected={index === selectedIndex}
          isDarkMode={isDarkMode}
          onClick={() => handleFileClick(file, index)}
        />
      ))}
      {hasMore && !isSearching && (
        <div ref={loadingTriggerRef} className="h-4">
          {isLoading && <div className="text-center">Loading...</div>}
        </div>
      )}
    </div>
  )
}

export default FileGrid
