import React, { useState, useEffect, useCallback } from 'react'

interface ResizablePanelProps {
  children: React.ReactNode
  minWidth?: number
  maxWidth?: number
  defaultWidth?: number
  className?: string
}

export const ResizablePanel: React.FC<ResizablePanelProps> = ({
  children,
  minWidth = 200,
  maxWidth = 600,
  defaultWidth = 256,
  className = ''
}) => {
  const [width, setWidth] = useState(defaultWidth)
  const [isResizing, setIsResizing] = useState(false)

  const handleMouseDown = (e: React.MouseEvent) => {
    e.preventDefault()
    setIsResizing(true)
  }

  const handleMouseMove = useCallback(
    (e: MouseEvent) => {
      if (!isResizing) return

      const newWidth = e.clientX
      if (newWidth >= minWidth && newWidth <= maxWidth) {
        setWidth(newWidth)
      }
    },
    [isResizing, minWidth, maxWidth]
  )

  const handleMouseUp = useCallback(() => {
    setIsResizing(false)
  }, [])

  useEffect(() => {
    if (isResizing) {
      document.addEventListener('mousemove', handleMouseMove)
      document.addEventListener('mouseup', handleMouseUp)
    }

    return () => {
      document.removeEventListener('mousemove', handleMouseMove)
      document.removeEventListener('mouseup', handleMouseUp)
    }
  }, [isResizing, handleMouseMove, handleMouseUp])

  return (
    <div
      className={`relative flex-shrink-0 ${className}`}
      style={{ width: width + 'px' }}
    >
      {children}
      <div
        className={`absolute right-0 top-0 bottom-0 w-1 cursor-col-resize hover:bg-blue-500 hover:bg-opacity-50 transition-colors
          ${isResizing ? 'bg-blue-500 bg-opacity-50' : ''}`}
        onMouseDown={handleMouseDown}
      />
    </div>
  )
}

export default ResizablePanel
