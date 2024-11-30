import { useResizable } from '@renderer/hooks/useResizable'
import React from 'react'

interface ResizablePanelProps {
  children: React.ReactNode
  initialWidth?: number
  minWidth?: number
  maxWidth?: number
}

export const ResizablePanel: React.FC<ResizablePanelProps> = ({
  children,
  initialWidth = 250,
  minWidth = 100,
  maxWidth = 400
}) => {
  const { width, startResizing } = useResizable(initialWidth, minWidth, maxWidth)

  return (
    <div className="relative flex h-full" style={{ width: `${width}px` }}>
      <div className="flex-1 overflow-y-auto border-r border-gray-200">
        {children}
      </div>
      <div
        className="absolute right-0 top-0 w-1 h-full cursor-col-resize hover:bg-blue-500 active:bg-blue-600"
        onMouseDown={startResizing}
      />
    </div>
  )
}

export default ResizablePanel
