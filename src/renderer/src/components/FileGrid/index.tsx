// Updated FileGrid component
import React, { useCallback, useEffect, useState } from 'react'
import { FileItem } from '../FileItem'
import { FileInfo } from '../../types/FileInfo'
import { useKeyBindings } from '../../keybinds/hooks'
import { KeyHandlerMap } from '../../keybinds/types'

interface FileGridProps {
  files: FileInfo[]
  directoryPath: string[]
  onDirectoryClick: (path: string[]) => void
  onFileClick: (file: FileInfo) => void
}

const FileGrid: React.FC<FileGridProps> = ({
  files,
  directoryPath,
  onDirectoryClick,
  onFileClick
}) => {
  const [selectedIndex, setSelectedIndex] = useState(0)
  const [gridColumns, setGridColumns] = useState(4)

  const handleFileClick = useCallback(
    (file: FileInfo) => {
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
    NAVIGATE_UP: () => navigateFiles('up'),
    NAVIGATE_DOWN: () => navigateFiles('down'),
    NAVIGATE_LEFT: () => navigateFiles('left'),
    NAVIGATE_RIGHT: () => navigateFiles('right'),
    SELECT_ITEM: () => {
      if (files[selectedIndex]) {
        handleFileClick(files[selectedIndex])
      }
    }
  }

  useKeyBindings(handlers)

  // Reset selection when files change
  useEffect(() => {
    setSelectedIndex(0)
  }, [directoryPath])

  return (
    <div className="file-grid grid grid-cols-4 gap-4 p-4">
      {files.map((file, index) => (
        <FileItem
          key={file.name}
          onClick={() => handleFileClick(file)}
          fileName={file.name}
          isDirectory={file.isDirectory}
          location={window.api.renderPath([...directoryPath, file.name])}
          isSelected={index === selectedIndex}
        />
      ))}
      {files.length === 0 && <div>No files found</div>}
    </div>
  )
}

export default FileGrid
