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
    <div
      className={`absolute z-50 min-w-[160px] py-1 rounded-md shadow-lg ${
        isDarkMode ? 'bg-gray-800 text-white' : 'bg-white text-gray-900'
      }`}
      style={{ top: y, left: x }}
    >
      {!isDirectory && (
        <button
          className={`text-xs font-semibold w-full px-2 py-1 text-left hover:${
            isDarkMode ? 'bg-gray-700' : 'bg-gray-100'
          } flex items-center gap-2`}
          onClick={(e) => {
            e.stopPropagation()
            onCopy()
          }}
        >
          <span>Copy</span>
          <span className="text-xs text-gray-500 ml-auto">Ctrl+C</span>
        </button>
      )}
      <button
        className={`text-xs font-semibold w-full px-2 py-1 text-left hover:${
          isDarkMode ? 'bg-gray-700' : 'bg-gray-100'
        } flex items-center gap-2`}
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
