import React from 'react'
import { useTheme } from '@renderer/context/ThemeContext'
import { Folder } from '@phosphor-icons/react'
import { useDirectoryOperations } from '../hooks/useDirectoryOperations'

interface EmptyStatePromptProps {
  onScanDirectory: () => void
}

const EmptyStatePrompt: React.FC<EmptyStatePromptProps> = ({ }) => {
  const { isDarkMode } = useTheme()
  const { handleDirectorySelection } = useDirectoryOperations()

  const handleScan = async () => {
    const directory = await window.api.openDirectoryPicker()
    if (directory) {
      const success = await handleDirectorySelection(directory)
      console.log('Directory selection result:', success)
    }
  }

  return (
    <div className="h-full w-full flex items-center justify-center">
      <div
        className={`
          text-center
          p-8
          rounded-lg
          border-2
          border-dashed
          ${isDarkMode ? 'border-gray-700' : 'border-gray-300'}
          max-w-md
        `}
      >
        <div
          className={`
            w-16 h-16
            mx-auto
            mb-4
            rounded-full
            flex
            items-center
            justify-center
            ${isDarkMode ? 'bg-gray-800' : 'bg-gray-100'}
          `}
        >
          <Folder className="w-8 h-8" />
        </div>
        <h2
          className={`
            text-xl
            font-semibold
            mb-2
            ${isDarkMode ? 'text-gray-200' : 'text-gray-800'}
          `}
        >
          No Files Found
        </h2>
        <p className={`mb-6 ${isDarkMode ? 'text-gray-400' : 'text-gray-600'}`}>
          Start by scanning a directory to index your files.
        </p>
        <button
          onClick={handleScan}
          className={`
            px-4
            py-2
            rounded
            bg-blue-500
            text-white
            hover:bg-blue-600
            transition-colors
            focus:outline-none
            focus:ring-2
            focus:ring-blue-500
            focus:ring-offset-2
            ${isDarkMode ? 'focus:ring-offset-gray-900' : 'focus:ring-offset-white'}
          `}
        >
          Select Directory
        </button>
      </div>
    </div>
  )
}

export default EmptyStatePrompt
