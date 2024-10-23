// Updated FileGrid component
import React, { useCallback, useEffect, useState } from 'react'
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
  isSearching?: boolean // New prop to distinguish between empty directory and no search results
}
const FileGrid: React.FC<FileGridProps> = ({
  files,
  directoryPath,
  onDirectoryClick,
  onFileClick,
  isSearching = false
}) => {
  const [selectedIndex, setSelectedIndex] = useState(0)
  const [gridColumns, setGridColumns] = useState(4)
  const { isDarkMode } = useTheme()
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

  useEffect(() => {
    const updateGridColumns = () => {
      const gridElement = document.querySelector('.file-grid')
      if (gridElement) {
        const computedStyle = window.getComputedStyle(gridElement)
        const columns = computedStyle.getPropertyValue('grid-template-columns').split(' ').length
        setGridColumns(columns)
      }
    }

    updateGridColumns()
    window.addEventListener('resize', updateGridColumns)
    return () => window.removeEventListener('resize', updateGridColumns)
  }, [])

  const navigateFiles = useCallback(
    (direction: 'up' | 'down' | 'left' | 'right') => {
      setSelectedIndex((prevIndex) => {
        let newIndex = prevIndex
        const maxIndex = files.length - 1

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

        // Ensure index stays within bounds
        if (newIndex < 0) newIndex = 0
        if (newIndex > maxIndex) newIndex = maxIndex

        return newIndex
      })
    },
    [gridColumns, files.length]
  )

  const handlers: KeyHandlerMap = {
    NAVIGATE_UP: () => {
      console.log('NAVIGATE_UP')
      navigateFiles('up')
    },
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
  }, [directoryPath])

  return (
    <div
      className={`grid grid-cols-4 xs:grid-cols-2 gap-2 p-4 auto-rows-fr ${
        isDarkMode ? 'bg-gray-900 text-gray-200' : 'bg-white text-gray-800'
      }`}
      tabIndex={0}
    >
      {files.map((file, index) => (
        <FileItem
          key={file.name}
          onClick={() => handleFileClick(file, index)}
          fileName={file.name}
          isDirectory={file.isDirectory}
          location={window.api.renderPath([...directoryPath, file.name])}
          isSelected={index === selectedIndex}
          isDarkMode={isDarkMode}
        />
      ))}
      {files.length === 0 && (
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
      )}
    </div>
  )
}

export default FileGrid
