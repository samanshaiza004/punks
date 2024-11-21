import React from 'react'
import FileIcons from '@renderer/components/FileIcons'
import { FileNode, DirectoryNode, FileSystemNode } from '../../../../types'
import * as ContextMenu from '@radix-ui/react-context-menu'

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
  const handleCopy = () => {
    console.log('Copy:', fileName)
  }

  const handleDelete = () => {
    console.log('Delete:', fileName)
  }

  const handleDragStart = (e: React.DragEvent<HTMLDivElement>) => {
    e.preventDefault()
    window.api.startDrag(window.api.renderPath([location, fileName]))
  }

  const handleContextMenu = () => {
    const baseNode = {
      id: Date.now(),
      name: fileName,
      path: window.api.renderPath([location, fileName]),
      last_modified: Date.now()
    }

    const node: FileSystemNode = isDirectory
      ? ({
          ...baseNode,
          type: 'directory',
          parent_path: location
        } as DirectoryNode)
      : ({
          ...baseNode,
          type: 'file',
          directory_path: location,
          hash: '',
          tags: []
        } as FileNode)

    // Implement showContextMenu functionality
    console.log('ContextMenu:', node)
  }

  const menuItemClass = `
    flex items-center px-2 py-1.5 text-sm outline-none cursor-default
    ${
      isDarkMode
        ? 'text-gray-200 focus:bg-gray-700 focus:text-gray-100 data-[highlighted]:bg-gray-700'
        : 'text-gray-700 focus:bg-gray-100 focus:text-gray-900 data-[highlighted]:bg-gray-100'
    }
  `

  return (
    <ContextMenu.Root>
      <ContextMenu.Trigger>
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
          <div
            className={`flex-shrink-0 ${isSelected ? 'text-white' : isDarkMode ? 'text-gray-400' : 'text-gray-500'}`}
          >
            <FileIcons fileName={fileName} isDirectory={isDirectory} />
          </div>
          <span className="truncate">{fileName}</span>
        </div>
      </ContextMenu.Trigger>

      <ContextMenu.Portal>
        <ContextMenu.Content
          className={`
            z-50 min-w-[160px] py-1 rounded-md shadow-lg
            ${isDarkMode ? 'bg-gray-800 border border-gray-700' : 'bg-white border border-gray-200'}
            animate-in fade-in-0 zoom-in-95
            data-[state=open]:animate-in
            data-[state=closed]:animate-out
            data-[state=closed]:fade-out-0
            data-[state=closed]:zoom-out-95
          `}
        >
          <ContextMenu.Item className={menuItemClass} onSelect={handleCopy}>
            Copy
          </ContextMenu.Item>
          <ContextMenu.Item className={menuItemClass} onSelect={handleDelete}>
            Delete
          </ContextMenu.Item>
        </ContextMenu.Content>
      </ContextMenu.Portal>
    </ContextMenu.Root>
  )
}
