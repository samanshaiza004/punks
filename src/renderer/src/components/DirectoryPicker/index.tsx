import React from 'react'
import { Button } from '../Button'
import { useDirectoryOperations } from '../../hooks/useDirectoryOperations'

interface DirectoryPickerProps {
  onDirectorySelected: (directory: string[]) => void
}

export const DirectoryPicker: React.FC<DirectoryPickerProps> = ({ onDirectorySelected }) => {
  const { handleDirectorySelection } = useDirectoryOperations()

  const handlePickDirectory = async () => {
    const directory = await window.api.openDirectoryPicker()
    if (directory) {
      await handleDirectorySelection(directory)
      onDirectorySelected([directory])
    }
  }

  return (
    <Button variant="primary" onClick={handlePickDirectory}>
      Pick Directory
    </Button>
  )
}
