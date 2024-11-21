import { useCallback } from 'react'
import { useToast } from '../context/ToastContext'

export const useDirectoryOperations = () => {
  const { showToast } = useToast()

  const handleDirectorySelection = useCallback(async (directory: string) => {
    try {
      // Start scanning the selected directory
      const scanStarted = await window.api.scanDirectory(directory)
      if (!scanStarted) {
        throw new Error('Failed to start directory scan')
      }

      // Save the selected directory path
      await window.electron.ipcRenderer.invoke('save-last-directory', directory)

      return true
    } catch (error: any) {
      console.error('Directory selection error:', error)
      showToast(`Error selecting directory: ${error.message}`, 'error')
      return false
    }
  }, [showToast])

  const getLastDirectory = useCallback(async () => {
    try {
      return await window.api.getLastSelectedDirectory()
    } catch (error) {
      console.error('Error getting last directory:', error)
      return null
    }
  }, [])

  return {
    handleDirectorySelection,
    getLastDirectory
  }
}
