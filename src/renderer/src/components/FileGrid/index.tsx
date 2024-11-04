import React, { useEffect, useState, useRef, useCallback, useMemo } from 'react'
import { FileItem } from '../FileItem'
import { FileInfo } from '@renderer/types/FileInfo'
import { useTheme } from '@renderer/context/ThemeContext'
import { useBatchLoading } from '@renderer/hooks/useBatchLoading'
import { useKeyBindings } from '@renderer/keybinds/hooks'
import { KeyHandlerMap } from '@renderer/types/types'
import { filterFiles } from '@renderer/utils/fileFilters'
import { FileFilterOptions } from '../FileFilter'

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
  const [gridColumns, setGridColumns] = useState(4)
  const { isDarkMode } = useTheme()
  const observerRef = useRef<IntersectionObserver | null>(null)
  const loadingTriggerRef = useRef<HTMLDivElement>(null)
  const gridRef = useRef<HTMLDivElement>(null)

  const updateGridColumns = useCallback(() => {
    if (gridRef.current) {
      const gridWidth = gridRef.current.offsetWidth
      // Calculate columns based on width
      // Using breakpoints similar to Tailwind's default breakpoints
      let columns = 4 // default max
      if (gridWidth < 640) columns = 1 // xs
      else if (gridWidth < 768) columns = 2 // sm
      else if (gridWidth < 1024) columns = 3 // md
      setGridColumns(columns)
    }
  }, [])

  const filteredFiles = useMemo(() => {
    const sourceFiles = isSearching ? searchResults : files
    return filterFiles(sourceFiles, fileFilters)
  }, [isSearching, searchResults, files, fileFilters])

  useEffect(() => {
    updateGridColumns()
    const resizeObserver = new ResizeObserver(updateGridColumns)
    if (gridRef.current) {
      resizeObserver.observe(gridRef.current)
    }

    return () => {
      resizeObserver.disconnect()
    }
  }, [updateGridColumns])

  useEffect(() => {
    if (isSearching) return

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

    return () => observerRef.current?.disconnect()
  }, [hasMore, isLoading, isSearching, loadMoreFiles])

  const displayedFiles = isSearching ? searchResults : files

  const navigateFiles = useCallback(
    (direction: 'up' | 'down' | 'left' | 'right') => {
      setSelectedIndex((prevIndex) => {
        let newIndex = prevIndex
        const maxIndex = displayedFiles.length - 1

        switch (direction) {
          case 'up':
            newIndex = prevIndex - gridColumns
            break
          case 'down':
            newIndex = prevIndex + gridColumns
            break
          case 'left':
            if (prevIndex % gridColumns !== 0) {
              newIndex = prevIndex - 1
            }
            break
          case 'right':
            if ((prevIndex + 1) % gridColumns !== 0 && prevIndex < maxIndex) {
              newIndex = prevIndex + 1
            }
            break
        }

        if (newIndex < 0) newIndex = 0
        if (newIndex > maxIndex) newIndex = maxIndex

        const selectedElement = document.querySelector(`[data-index="${newIndex}"]`)
        selectedElement?.scrollIntoView({ block: 'nearest', behavior: 'smooth' })

        return newIndex
      })
    },
    [gridColumns, displayedFiles.length]
  )

  const handlers: KeyHandlerMap = useMemo(
    () => ({
      NAVIGATE_UP: () => navigateFiles('up'),
      NAVIGATE_DOWN: () => navigateFiles('down'),
      NAVIGATE_LEFT: () => navigateFiles('left'),
      NAVIGATE_RIGHT: () => navigateFiles('right'),
      SELECT_ITEM: (): void => {
        const selectedFile = displayedFiles[selectedIndex]
        if (selectedFile) {
          if (selectedFile.isDirectory) {
            onDirectoryClick([...directoryPath, selectedFile.name])
          } else {
            onFileClick(selectedFile)
          }
        }
      }
    }),
    [navigateFiles, displayedFiles, selectedIndex, directoryPath, onDirectoryClick, onFileClick]
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

  return (
    <div className="relative w-full h-full overflow-auto">
      <div
        ref={gridRef}
        className={`
          grid
          grid-cols-1
          sm:grid-cols-2
          lg:grid-cols-3
          xl:grid-cols-4
          auto-rows-fr
          file-grid
          ${isDarkMode ? 'bg-gray-900 text-gray-200' : 'bg-white text-gray-800'}
        `}
      >
        {filteredFiles.map((file, index) => (
          <FileItem
            key={`${file.name}-${index}`}
            data-index={index}
            onClick={() => {
              setSelectedIndex(index)
              if (file.isDirectory) {
                onDirectoryClick([...directoryPath, file.name])
              } else {
                onFileClick(file)
              }
            }}
            fileName={file.name}
            isDirectory={file.isDirectory}
            location={file.location}
            isSelected={index === selectedIndex}
            isDarkMode={isDarkMode}
          />
        ))}

        {!isSearching && hasMore && (
          <div ref={loadingTriggerRef} className="col-span-full flex justify-center p-4">
            {isLoading ? (
              <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-gray-500" />
            ) : (
              <p className="text-sm text-gray-500">
                Showing {filteredFiles.length} of {totalFiles} items
              </p>
            )}
          </div>
        )}
      </div>
    </div>
  )
}

export default FileGrid