import React, { ButtonHTMLAttributes, useState } from 'react'
import FileIcons from '../FileIcons'
import ContextMenu from '../ContextMenu'
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
  const [contextMenu, setContextMenu] = useState<{
    x: number
    y: number
  } | null>(null)

  const handleContextMenu = (e: React.MouseEvent) => {
    e.preventDefault()
    setContextMenu({ x: e.clientX, y: e.clientY })
  }

  const handleDelete = async () => {
    const confirmed = window.confirm(`Are you sure you want to delete ${fileName}?`)
    if (confirmed) {
      try {
        await window.api.deleteFile(location)
      } catch (err) {
        console.error('Error deleting file:', err)
      }
    }
    setContextMenu(null)
  }
  const handleDragStart = (e: React.DragEvent<HTMLButtonElement>) => {
    e.preventDefault()
    window.api.startDrag(location)
  }

  const handleCopy = async () => {
    try {
      await window.api.copyFileToClipboard(location)
    } catch (err) {
      console.error('Error copying file to clipboard:', err)
    }
    setContextMenu(null)
  }
  return (
    <button
      className={`
        w-full
        px-2
        py-3
        rounded-sm
        flex 
        items-center 
        justify-start 
        space-x-2 
        transition-colors
        focus:outline-none 
        focus:ring-2
        min-w-[200px] min-h-[50px] 
        ${
          isSelected
            ? isDarkMode
              ? 'bg-blue-600 text-white'
              : 'bg-blue-500 text-white'
            : isDarkMode
              ? 'text-gray-200 hover:bg-gray-800'
              : 'text-gray-800 hover:bg-gray-100'
        }
        ${isDarkMode ? 'focus:ring-blue-500' : 'focus:ring-blue-400'}
      `}
      draggable={!isDirectory}
      onDragStart={handleDragStart}
      {...props}
      onContextMenu={handleContextMenu}
    >
      <div className="flex-shrink-0">
        <FileIcons fileName={fileName} isDirectory={isDirectory} />
      </div>
      <span className="text-sm truncate" title={fileName}>
        {fileName}
      </span>
      {contextMenu && (
        <ContextMenu
          x={contextMenu.x}
          y={contextMenu.y}
          onClose={() => setContextMenu(null)}
          onDelete={handleDelete}
          onCopy={handleCopy}
          isDirectory={isDirectory}
        />
      )}
    </button>
  )
}
