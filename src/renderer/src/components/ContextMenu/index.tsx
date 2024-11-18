import React from 'react'
import * as ContextMenu from '@radix-ui/react-context-menu'
import { useTheme } from '@renderer/context/ThemeContext'
import { FileInfo } from '@renderer/types/FileInfo'

interface FileContextMenuProps {
  children: React.ReactNode
  file: FileInfo
  onCopy?: () => void
  onDelete?: () => void
}

const FileContextMenu: React.FC<FileContextMenuProps> = ({
  children,
  file,
  onCopy,
  onDelete
}) => {
  const { isDarkMode } = useTheme()

  const menuItemClass = `
    text-xs font-semibold w-full px-2 py-1.5 outline-none
    flex items-center gap-2
    ${isDarkMode 
      ? 'text-gray-100 focus:bg-gray-700 data-[highlighted]:bg-gray-700' 
      : 'text-gray-900 focus:bg-gray-100 data-[highlighted]:bg-gray-100'
    }
  `

  const handleDelete = async () => {
    const confirmed = window.confirm(`Are you sure you want to delete ${file.name}?`)
    if (confirmed) {
      try {
        await window.api.deleteFile(file.location)
        onDelete?.()
      } catch (err) {
        console.error('Error deleting file:', err)
      }
    }
  }

  const handleCopy = async () => {
    try {
      await window.api.copyFileToClipboard(file.location)
      onCopy?.()
    } catch (err) {
      console.error('Error copying file to clipboard:', err)
    }
  }

  return (
    <ContextMenu.Root>
      <ContextMenu.Trigger asChild>
        {children}
      </ContextMenu.Trigger>

      <ContextMenu.Portal>
        <ContextMenu.Content
          className={`
            min-w-[160px] py-1 rounded-md shadow-lg
            ${isDarkMode ? 'bg-gray-800 border border-gray-700' : 'bg-white border border-gray-200'}
            animate-in fade-in-0 zoom-in-95
            data-[state=open]:animate-in
            data-[state=closed]:animate-out
            data-[state=closed]:fade-out-0
            data-[state=closed]:zoom-out-95
            data-[side=top]:slide-in-from-bottom-2
            data-[side=bottom]:slide-in-from-top-2
          `}
        >
          {!file.isDirectory && (
            <ContextMenu.Item
              className={menuItemClass}
              onSelect={handleCopy}
            >
              <span>Copy</span>
              <span className="text-xs text-gray-500 ml-auto">Ctrl+C</span>
            </ContextMenu.Item>
          )}
          
          <ContextMenu.Item
            className={menuItemClass}
            onSelect={handleDelete}
          >
            <span>Delete</span>
            <span className="text-xs text-gray-500 ml-auto">Del</span>
          </ContextMenu.Item>
        </ContextMenu.Content>
      </ContextMenu.Portal>
    </ContextMenu.Root>
  )
}

export default FileContextMenu
