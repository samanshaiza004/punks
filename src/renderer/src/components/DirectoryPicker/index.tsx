import React from 'react'

interface DirectoryPickerProps {
  onDirectorySelected: (directory: string[]) => void
}

export const DirectoryPicker: React.FC<DirectoryPickerProps> = ({ onDirectorySelected }) => {
  const handlePickDirectory = async () => {
    const directory = await window.api.openDirectoryPicker()
    if (directory) {
      onDirectorySelected([directory])
    }
  }

  return (
    <button
      className="px-1 hover:bg-gray-100 focus:outline-none focus:ring-2 focus:ring-blue-500"
      onClick={handlePickDirectory}
    >
      Pick Directory
    </button>
  )
}
