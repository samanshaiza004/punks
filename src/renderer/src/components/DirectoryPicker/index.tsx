import React from 'react'
import { Button } from '../Button'

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
    <Button variant="primary" onClick={handlePickDirectory}>
      Pick Directory
    </Button>
  )
}
