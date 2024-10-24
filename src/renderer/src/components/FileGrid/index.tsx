import React, { useCallback, useEffect, useState, useRef } from 'react'
import { FileItem } from '../FileItem'
import { FileInfo } from '../../types/FileInfo'
import { useKeyBindings } from '../../keybinds/hooks'
import { KeyHandlerMap } from '../../types/types'
import { useTheme } from '@renderer/context/ThemeContext'

interface FileGridProps {
  files: FileInfo[]
  directoryPath: string[]
  onDirectoryClick: (path: string[]) => void
  onFileClick: (file: FileInfo) => void
  isSearching?: boolean
}

const ITEM_HEIGHT = 120 // Height of each grid item in pixels
const ITEM_WIDTH = 120 // Width of each grid item in pixels
const BUFFER_SIZE = 5 // Number of extra rows to render above and below viewport

const FileGrid: React.FC<FileGridProps> = ({
  files,
  directoryPath,
  onDirectoryClick,
  onFileClick,
  isSearching = false
}) => {
  const { isDarkMode } = useTheme()
  const containerRef = useRef<HTMLDivElement>(null)
  const [selectedIndex, setSelectedIndex] = useState(0)
  const [scrollTop, setScrollTop] = useState(0)
  const [containerWidth, setContainerWidth] = useState(0)
  const [visibleItems, setVisibleItems] = useState<FileInfo[]>([])

  // Calculate grid dimensions
  const columnsCount = Math.max(1, Math.floor(containerWidth / ITEM_WIDTH))
  const totalRows = Math.ceil(files.length / columnsCount)
  const totalHeight = totalRows * ITEM_HEIGHT

  // Calculate which items should be visible
  const calculateVisibleItems = useCallback(() => {
    if (!containerRef.current) return

    const visibleStart = Math.max(0, Math.floor(scrollTop / ITEM_HEIGHT) - BUFFER_SIZE)
    const visibleEnd = Math.min(
      totalRows,
      Math.ceil((scrollTop + containerRef.current.clientHeight) / ITEM_HEIGHT) + BUFFER_SIZE
    )

    const startIndex = visibleStart * columnsCount
    const endIndex = Math.min(files.length, visibleEnd * columnsCount)

    const items = files.slice(startIndex, endIndex)
    setVisibleItems(items)
  }, [files, scrollTop, columnsCount, totalRows])

  // Handle scroll events
  const handleScroll = useCallback((event: React.UIEvent<HTMLDivElement>) => {
    const target = event.target as HTMLDivElement
    setScrollTop(target.scrollTop)
  }, [])

  // Handle resize events
  useEffect(() => {
    const updateContainerWidth = () => {
      if (containerRef.current) {
        setContainerWidth(containerRef.current.clientWidth)
      }
    }

    updateContainerWidth()
    const observer = new ResizeObserver(updateContainerWidth)
    if (containerRef.current) {
      observer.observe(containerRef.current)
    }

    return () => observer.disconnect()
  }, [])

  // Update visible items when necessary
  useEffect(() => {
    calculateVisibleItems()
  }, [calculateVisibleItems, scrollTop, containerWidth])

  const handleFileClick = useCallback(
    (file: FileInfo, index: number) => {
      setSelectedIndex(index)
      if (file.isDirectory) {
        onDirectoryClick([...directoryPath, file.name])
      } else {
        onFileClick(file)
      }
    },
    [directoryPath, onDirectoryClick, onFileClick]
  )

  const navigateFiles = useCallback(
    (direction: 'up' | 'down' | 'left' | 'right') => {
      setSelectedIndex((prevIndex) => {
        let newIndex = prevIndex
        const maxIndex = files.length - 1

        switch (direction) {
          case 'up':
            newIndex = prevIndex - columnsCount
            break
          case 'down':
            newIndex = prevIndex + columnsCount
            break
          case 'left':
            if (prevIndex % columnsCount !== 0) {
              newIndex = prevIndex - 1
            }
            break
          case 'right':
            if ((prevIndex + 1) % columnsCount !== 0 && prevIndex < maxIndex) {
              newIndex = prevIndex + 1
            }
            break
        }

        // Ensure index stays within bounds
        if (newIndex < 0) newIndex = 0
        if (newIndex > maxIndex) newIndex = maxIndex

        // Scroll selected item into view
        const itemRow = Math.floor(newIndex / columnsCount)
        const itemOffset = itemRow * ITEM_HEIGHT
        if (containerRef.current) {
          if (itemOffset < scrollTop) {
            containerRef.current.scrollTop = itemOffset
          } else if (itemOffset + ITEM_HEIGHT > scrollTop + containerRef.current.clientHeight) {
            containerRef.current.scrollTop =
              itemOffset - containerRef.current.clientHeight + ITEM_HEIGHT
          }
        }

        return newIndex
      })
    },
    [columnsCount, files.length]
  )

  const handlers: KeyHandlerMap = {
    NAVIGATE_UP: () => navigateFiles('up'),
    NAVIGATE_DOWN: () => navigateFiles('down'),
    NAVIGATE_LEFT: () => navigateFiles('left'),
    NAVIGATE_RIGHT: () => navigateFiles('right'),
    SELECT_ITEM: () => {
      if (files[selectedIndex]) {
        handleFileClick(files[selectedIndex], selectedIndex)
      }
    }
  }

  useKeyBindings(handlers)

  // Reset selection when files change
  useEffect(() => {
    setSelectedIndex(0)
    setScrollTop(0)
    if (containerRef.current) {
      containerRef.current.scrollTop = 0
    }
  }, [directoryPath])

  if (files.length === 0) {
    return (
      <div
        className={`col-span-full flex flex-col items-center justify-center p-8 rounded-lg ${
          isDarkMode ? 'bg-gray-700 text-gray-200' : 'bg-gray-50 text-gray-600'
        }`}
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
    <div
      ref={containerRef}
      /* className={`h-full overflow-auto ${isDarkMode ? 'bg-gray-900 text-gray-200' : 'bg-white text-gray-800'}`} */
      onScroll={handleScroll}
      tabIndex={0}
    >
      <div
      /* style={{
          height: totalHeight,
          position: 'relative'
        }} */
      >
        <div
          className="grid gap-2 p-4 absolute w-full"
          /* style={{
            gridTemplateColumns: `repeat(${columnsCount}, minmax(0, 1fr))`,
            transform: `translateY(${Math.floor(scrollTop / ITEM_HEIGHT) * ITEM_HEIGHT}px)`
          }} */
        >
          {visibleItems.map((file, index) => {
            const actualIndex = Math.floor(scrollTop / ITEM_HEIGHT) * columnsCount + index
            return (
              <FileItem
                key={`${file.name}-${actualIndex}`}
                onClick={() => handleFileClick(file, actualIndex)}
                fileName={file.name}
                isDirectory={file.isDirectory}
                location={window.api.renderPath([...directoryPath, file.name])}
                isSelected={actualIndex === selectedIndex}
                isDarkMode={isDarkMode}
              />
            )
          })}
        </div>
      </div>
    </div>
  )
}

export default FileGrid
