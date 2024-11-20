import React, { createContext, useContext, useState, ReactNode } from 'react'
import ContextMenu from '@renderer/components/ContextMenu'
import { FileNode } from '@renderer/types/FileNode'

interface ContextMenuContextType {
  showContextMenu: (file: FileNode, event: React.MouseEvent) => void
  hideContextMenu: () => void
}

const ContextMenuContext = createContext<ContextMenuContextType | undefined>(undefined)

export function ContextMenuProvider({ children }: { children: ReactNode }) {
  const [menuState, setMenuState] = useState<{
    x: number
    y: number
    file?: FileNode
    isVisible: boolean
  }>({
    x: 0,
    y: 0,
    isVisible: false
  })

  const showContextMenu = (file: FileNode, event: React.MouseEvent) => {
    event.preventDefault()
    setMenuState({
      x: event.clientX,
      y: event.clientY,
      file,
      isVisible: true
    })
  }

  const hideContextMenu = () => {
    setMenuState(prev => ({ ...prev, isVisible: false }))
  }

  const handleCopy = () => {
    // Implement file copy logic
    hideContextMenu()
  }

  const handleDelete = () => {
    // Implement file delete logic
    hideContextMenu()
  }

  return (
    <ContextMenuContext.Provider value={{ showContextMenu, hideContextMenu }}>
      {children}
      {menuState.isVisible && menuState.file && (
        <ContextMenu
          x={menuState.x}
          y={menuState.y}
          onClose={hideContextMenu}
          onCopy={handleCopy}
          onDelete={handleDelete}
          isDirectory={menuState.file.isDirectory}
        />
      )}
    </ContextMenuContext.Provider>
  )
}

export function useContextMenu() {
  const context = useContext(ContextMenuContext)
  if (!context) {
    throw new Error('useContextMenu must be used within a ContextMenuProvider')
  }
  return context
}
