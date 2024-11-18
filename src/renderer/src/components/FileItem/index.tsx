import React, { ButtonHTMLAttributes } from 'react'
import FileIcons from '../FileIcons'
import { useContextMenu } from '@renderer/context/ContextMenuContext'

type FileItemProps = {
  fileName: string
  isDirectory: boolean
  location: string
  isSelected: boolean
  isDarkMode: boolean
} & ButtonHTMLAttributes<HTMLButtonElement>

export function FileItem({
  fileName,
  isDirectory,
  location,
  isSelected,
  isDarkMode,
  ...props
}: FileItemProps) {
  const { showContextMenu } = useContextMenu()

  const handleDragStart = (e: React.DragEvent<HTMLButtonElement>) => {
    e.preventDefault()
    window.api.startDrag(window.api.renderPath([location, fileName]))
  }

  const handleContextMenu = (e: React.MouseEvent<HTMLButtonElement>) => {
    showContextMenu(
      {
        name: fileName,
        location,
        isDirectory
      },
      e
    )
  }

  return (
    <button
      className={`
        min-w-[200px]
        h-[35px]
        px-2
        rounded-sm
        flex 
        items-center 
        gap-2
        transition-colors
        focus:outline-none 
        focus:ring-2
        text-sm
        whitespace-nowrap
        ${
          isSelected
            ? isDarkMode
              ? 'bg-blue-600 text-white'
              : 'bg-blue-500 text-white'
            : isDarkMode
            ? 'text-gray-300 hover:bg-gray-700'
            : 'text-gray-700 hover:bg-gray-100'
        }
        ${
          isDarkMode
            ? 'focus:ring-blue-500 focus:ring-offset-gray-800'
            : 'focus:ring-blue-400 focus:ring-offset-white'
        }
      `}
      draggable
      onDragStart={handleDragStart}
      onContextMenu={handleContextMenu}
      title={fileName}
      {...props}
    >
      <div className="flex-shrink-0">
        <FileIcons fileName={fileName} isDirectory={isDirectory} />
      </div>
      <span className="truncate">{fileName}</span>
    </button>
  )
}
