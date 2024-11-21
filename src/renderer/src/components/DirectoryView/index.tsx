import React from 'react'
import { FileAddressItem } from '../FileAddressItem/FileAddressItem'
import { KeyHandlerMap } from '@renderer/types/keybinds'
import { useKeyBindings } from '@renderer/keybinds/hooks'
import { useTheme } from '@renderer/context/ThemeContext'

interface DirectoryViewProps {
  directoryPath: string[]
  onDirectoryClick: (path: string[]) => void
}

const DirectoryView: React.FC<DirectoryViewProps> = ({ directoryPath, onDirectoryClick }) => {
  const { isDarkMode } = useTheme()

  const handleClick = (index: number) => {
    onDirectoryClick(directoryPath.slice(0, index + 1))
  }

  const goBack = () => {
    if (directoryPath.length > 1) {
      onDirectoryClick(directoryPath.slice(0, -1))
    }
  }

  const handlers: KeyHandlerMap = {
    GO_BACK: () => {
      goBack()
    }
  }

  useKeyBindings(handlers)

  return (
    <div
      className={`flex items-center space-x-1 px-2 py-1 overflow-x-auto rounded-sm ${
        isDarkMode ? 'bg-gray-800 text-gray-200' : 'bg-gray-100 text-gray-800'
      }`}
    >
      {directoryPath.map((directory, index) => (
        <React.Fragment key={index}>
          <FileAddressItem
            onClick={() => handleClick(index)}
            fileName={directory}
            isDarkMode={isDarkMode}
          />
        </React.Fragment>
      ))}
    </div>
  )
}

export default DirectoryView
