import { useTheme } from '@renderer/context/ThemeContext'
import { useEffect } from 'react'

interface ContextMenuProps {
  x: number
  y: number
  onClose: () => void
  onDelete: () => void
  onCopy: () => void
  isDirectory: boolean
}

const ContextMenu: React.FC<ContextMenuProps> = ({
  x,
  y,
  onClose,
  onDelete,
  onCopy,
  isDirectory
}) => {
  const { isDarkMode } = useTheme()

  useEffect(() => {
    const handleClick = () => onClose()
    window.addEventListener('click', handleClick)
    return () => window.removeEventListener('click', handleClick)
  }, [onClose])

  return (
    <div>
      {!isDirectory && (
        <button
          className={`w-full px-4 py-2 text-left hover:${
            isDarkMode ? 'bg-gray-700' : 'bg-gray-100'
          }`}
          onClick={(e) => {
            e.stopPropagation()
            onCopy()
          }}
        >
          Copy
        </button>
      )}
      <button
        className={`w-full px-4 py-2 text-left text-red-600 hover:${
          isDarkMode ? 'bg-gray-700' : 'bg-gray-100'
        }`}
        onClick={(e) => {
          e.stopPropagation()
          onDelete()
        }}
      >
        Delete
      </button>
    </div>
  )
}

export default ContextMenu
