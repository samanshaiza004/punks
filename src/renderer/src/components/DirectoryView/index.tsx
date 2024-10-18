import React from 'react'
import { FileAddressItem } from '../FileAddressItem/FileAddressItem'
import { useHotkeys } from 'react-hotkeys-hook'

interface DirectoryViewProps {
  directoryPath: string[]
  onDirectoryClick: (path: string[]) => void
}

const DirectoryView: React.FC<DirectoryViewProps> = ({ directoryPath, onDirectoryClick }) => {
  const handleClick = (index: number) => {
    onDirectoryClick(directoryPath.slice(0, index + 1))
  }
  const goBack = () => {
    if (directoryPath.length > 1) {
      onDirectoryClick(directoryPath.slice(0, -1))
    }
  }

  // Handle Alt+Left Arrow and Alt+H for going back
  useHotkeys('alt+left, alt+h', (event) => {
    event.preventDefault() // Prevent browser back navigation
    goBack()
  })

  return (
    <div className="flex items-center space-x-1 p-2">
      {directoryPath.map((directory, index) => (
        <React.Fragment key={index}>
          {index > 0 && <span className="text-gray-400">/</span>}
          <FileAddressItem onClick={() => handleClick(index)} fileName={directory} />
        </React.Fragment>
      ))}
    </div>
  )
}

export default DirectoryView
