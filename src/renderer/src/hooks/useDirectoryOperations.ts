import { useCallback, useEffect, useRef } from 'react'
import { useToast } from '../context/ToastContext'
import { useScanProgress } from '../context/ScanProgressContext'
import type { ScanProgress, FileNode } from '../../../types/index'

interface ExtendedScanProgress extends ScanProgress {
  type?: 'directory' | 'file'
  path?: string
  batch?: FileNode[]
}

export const useDirectoryOperations = () => {
  const { showToast } = useToast()
  const { startScanning, updateProgress, completeScan, addBatchResults } = useScanProgress()
  const scanAbortController = useRef<AbortController | null>(null)

  useEffect(() => {
    const handleScanProgress = (progress: ExtendedScanProgress) => {
      updateProgress({
        total: progress.total,
        processed: progress.processed,
        percentComplete: progress.percentComplete,
        type: progress.type,
        path: progress.path,
        batch: progress.batch
      })
      
      if (progress.batch && progress.batch.length > 0) {
        addBatchResults(progress.batch)
      }
    }

    const handleScanComplete = () => {
      completeScan()
      showToast('Directory scan completed successfully', 'success')
      window.dispatchEvent(new CustomEvent('directoryScanned'))
    }

    const handleScanError = (error: string) => {
      completeScan()
      showToast(`Scan error: ${error}`, 'error')
    }

    window.api.onScanProgress(handleScanProgress)
    window.api.onScanComplete(handleScanComplete)
    window.api.onScanError(handleScanError)

    return () => {
      window.electron.ipcRenderer.removeAllListeners('scan-progress')
      window.electron.ipcRenderer.removeAllListeners('scan-complete')
      window.electron.ipcRenderer.removeAllListeners('scan-error')
      
      if (scanAbortController.current) {
        scanAbortController.current.abort()
        scanAbortController.current = null
      }
    }
  }, [updateProgress, completeScan, showToast, addBatchResults])

  const handleDirectorySelection = useCallback(async (directory: string) => {
    try {
      if (scanAbortController.current) {
        scanAbortController.current.abort()
      }
      scanAbortController.current = new AbortController()

      startScanning()
      
      // Save the selected directory path
      await window.electron.ipcRenderer.invoke('save-last-directory', directory)
      
      await window.api.scanDirectory(directory, scanAbortController.current.signal)
    } catch (error: any) {
      if (error.name === 'AbortError') {
        showToast('Scan cancelled', 'info')
      } else {
        showToast(`Failed to scan directory: ${error.message}`, 'error')
      }
      completeScan()
    }
  }, [startScanning, completeScan, showToast])

  const getLastDirectory = useCallback(async () => {
    try {
      return await window.electron.ipcRenderer.invoke('get-last-directory')
    } catch (error) {
      console.error('Error getting last directory:', error)
      return null
    }
  }, [])

  const cancelScan = useCallback(() => {
    if (scanAbortController.current) {
      scanAbortController.current.abort()
      scanAbortController.current = null
      completeScan()
      showToast('Scan cancelled', 'info')
    }
  }, [completeScan, showToast])

  return {
    handleDirectorySelection,
    cancelScan,
    getLastDirectory
  }
}
