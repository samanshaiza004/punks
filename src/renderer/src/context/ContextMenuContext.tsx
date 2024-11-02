import { FileInfo } from '@renderer/types/FileInfo'
import { createContext, useContext, useEffect, useState } from 'react'
import { useTheme } from './ThemeContext'

interface ContextMenuState {
  isOpen: boolean
  x: number
  y: number
  file: FileInfo | null
}

interface ContextMenuContextType {
  menuState: ContextMenuState
  openMenu: (x: number, y: number, file: FileInfo) => void
  closeMenu: () => void
}

const ContextMenuContext = createContext<ContextMenuContextType | undefined>(undefined)

export function ContextMenuProvider({ children }: { children: React.ReactNode }) {
  const [menuState, setMenuState] = useState<ContextMenuState>({
    isOpen: false,
    x: 0,
    y: 0,
    file: null
  })

  const openMenu = (x: number, y: number, file: FileInfo) => {
    const viewportHeight = window.innerHeight
    const viewportWidth = window.innerWidth

    const menuHeight = 100
    const menuWidth = 100

    const adjustedY = y + menuHeight > viewportHeight ? viewportHeight - menuHeight : y
    const adjustedX = x + menuWidth > viewportWidth ? viewportWidth - menuWidth : x

    setMenuState({
      isOpen: true,
      x: adjustedX,
      y: adjustedY,
      file
    })
  }

  const closeMenu = () => {
    setMenuState({
      isOpen: false,
      x: 0,
      y: 0,
      file: null
    })
  }

  return (
    <ContextMenuContext.Provider value={{ menuState, openMenu, closeMenu }}>
      {children}
      {menuState.isOpen && menuState.file && (
        <GlobalContextMenu
          x={menuState.x}
          y={menuState.y}
          file={menuState.file}
          onClose={closeMenu}
        />
      )}
    </ContextMenuContext.Provider>
  )
}

export function useContextMenu() {
  const context = useContext(ContextMenuContext)
  if (context === undefined) {
    throw new Error('useContextMenu must be used within a ContextMenuProvider, duh')
  }
  return context
}

function GlobalContextMenu({
  x,
  y,
  file,
  onClose
}: {
  x: number
  y: number
  file: { name: string; location: string; isDirectory: boolean }
  onClose: () => void
}) {
  const { isDarkMode } = useTheme()

  const handleDelete = async () => {
    const confirmed = window.confirm(`Are you sure you want to delete ${file.name}?`)
    if (confirmed) {
      try {
        await window.api.deleteFile(file.location)
      } catch (err) {
        console.error('Error deleting file:', err)
      }
    }
    onClose()
  }

  const handleCopy = async () => {
    try {
      await window.api.copyFileToClipboard(file.location)
    } catch (err) {
      console.error('Error copying file to clipboard:', err)
    }
    onClose()
  }

  useEffect(() => {
    const handleClick = () => onClose()
    window.addEventListener('click', handleClick)
    return () => window.removeEventListener('click', handleClick)
  }, [onClose])

  return (
    <div
      className={`fixed z-50 min-w-[160px] py-1 rounded-md shadow-lg ${
        isDarkMode ? 'bg-gray-800 text-white' : 'bg-white text-gray-900'
      }`}
      style={{ top: y, left: x }}
      onClick={(e) => e.stopPropagation()}
    >
      {!file.isDirectory && (
        <button
          className={`text-xs font-semibold w-full px-2 py-1 text-left hover:${
            isDarkMode ? 'bg-gray-700' : 'bg-gray-100'
          } flex items-center gap-2`}
          onClick={handleCopy}
        >
          <span>Copy</span>
          <span className="text-xs text-gray-500 ml-auto">Ctrl+C</span>
        </button>
      )}
      <button
        className={`text-xs font-semibold w-full px-2 py-1 text-left hover:${
          isDarkMode ? 'bg-gray-700' : 'bg-gray-100'
        } flex items-center gap-2`}
        onClick={handleDelete}
      >
        Delete
      </button>
    </div>
  )
}
