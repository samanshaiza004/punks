import React from 'react'
import FileIcons from '@renderer/components/FileIcons'
import { useContextMenu } from '@renderer/context/ContextMenuContext'

interface FileItemProps {
  fileName: string
  isDirectory: boolean
  location: string
  isSelected: boolean
  isDarkMode: boolean
  onClick: () => void
}

export const FileItem: React.FC<FileItemProps> = ({
  fileName,
  isDirectory,
  location,
  isSelected,
  isDarkMode,
  onClick
}) => {
  const { showContextMenu } = useContextMenu()

  const handleContextMenu = (e: React.MouseEvent<HTMLDivElement>) => {
    showContextMenu(
      {
        name: fileName,
        location,
        isDirectory
      },
      e
    )
  }

  const handleDragStart = (e: React.DragEvent<HTMLDivElement>) => {
    e.preventDefault()
    window.api.startDrag(window.api.renderPath([location, fileName]))
  }

  return (
    <div
      className={`
        flex
        items-center
        gap-2
        px-2
        py-1
        cursor-pointer
        truncate
        transition-colors
        duration-100
        ${
          isSelected
            ? 'bg-blue-500 text-white'
            : isDarkMode
            ? 'hover:bg-gray-700 text-gray-200'
            : 'hover:bg-gray-100 text-gray-800'
        }
      `}
      title={`${fileName}\n${location}`}
      onClick={onClick}
      onContextMenu={handleContextMenu}
      draggable
      onDragStart={handleDragStart}
    >
      <div className={`flex-shrink-0 ${isSelected ? 'text-white' : isDarkMode ? 'text-gray-400' : 'text-gray-500'}`}>
        <FileIcons fileName={fileName} isDirectory={isDirectory} />
      </div>
      <span className="truncate">{fileName}</span>
    </div>
  )
}
