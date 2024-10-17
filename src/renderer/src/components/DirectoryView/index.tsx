import React from 'react'
import { FileAddressItem } from '../FileAddressItem/FileAddressItem'

interface DirectoryViewProps {
  directoryPath: string[]
  onDirectoryClick: (path: string[]) => void
}

const DirectoryView: React.FC<DirectoryViewProps> = ({ directoryPath, onDirectoryClick }) => {
  const handleClick = (index: number) => {
    onDirectoryClick(directoryPath.slice(0, index + 1))
  }

  return (
    <div className="flex-row flex sticky gap-1">
      {directoryPath.map((directory, index) => (
        <FileAddressItem key={index} onClick={() => handleClick(index)} fileName={directory} />
      ))}
    </div>
  )
}

export default DirectoryView
