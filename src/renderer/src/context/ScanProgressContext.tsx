import React, { createContext, useContext, useState, useCallback, useRef } from 'react'

interface ScanProgress {
  total: number
  processed: number
  percentComplete: number
  type?: 'directory' | 'file'
  path?: string
  batch?: any[]
}

interface ScanProgressContextType {
  isScanning: boolean
  progress: ScanProgress | null
  startScanning: () => void
  updateProgress: (progress: ScanProgress) => void
  completeScan: () => void
  addBatchResults: (batch: any[]) => void
  directoryStructure: Map<string, string[]>
}

const ScanProgressContext = createContext<ScanProgressContextType | undefined>(undefined)

export const ScanProgressProvider: React.FC<{ children: React.ReactNode }> = ({ children }) => {
  const [isScanning, setIsScanning] = useState(false)
  const [progress, setProgress] = useState<ScanProgress | null>(null)
  const [directoryStructure, setDirectoryStructure] = useState<Map<string, string[]>>(new Map())
  const batchQueue = useRef<any[][]>([])
  const processingBatch = useRef(false)

  const startScanning = useCallback(() => {
    setIsScanning(true)
    setProgress(null)
    setDirectoryStructure(new Map())
    batchQueue.current = []
    processingBatch.current = false
  }, [])

  const processBatchQueue = useCallback(async () => {
    if (processingBatch.current || batchQueue.current.length === 0) return

    processingBatch.current = true
    const batch = batchQueue.current.shift()!

    // Process batch in smaller chunks to keep UI responsive
    const chunkSize = 20
    for (let i = 0; i < batch.length; i += chunkSize) {
      const chunk = batch.slice(i, i + chunkSize)
      const event = new CustomEvent('filesAdded', {
        detail: {
          files: chunk,
          batchId: `batch-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`
        }
      })
      window.dispatchEvent(event)
      // Give UI time to update
      await new Promise(resolve => setTimeout(resolve, 16))
    }

    processingBatch.current = false
    // Process next batch if available
    processBatchQueue()
  }, [])

  const updateProgress = useCallback((newProgress: ScanProgress) => {
    setProgress(prev => {
      // Only update if the new progress is different
      if (!prev || 
          prev.processed !== newProgress.processed || 
          prev.total !== newProgress.total ||
          prev.type !== newProgress.type) {
        return newProgress
      }
      return prev
    })

    // Update directory structure if this is a directory update
    if (newProgress.type === 'directory' && newProgress.path) {
      setDirectoryStructure(prev => {
        const next = new Map(prev)
        const parentPath = newProgress.path!.split('/').slice(0, -1).join('/')
        if (parentPath) {
          const siblings = next.get(parentPath) || []
          if (!siblings.includes(newProgress.path!)) {
            next.set(parentPath, [...siblings, newProgress.path!])
          }
        }
        return next
      })
    }
  }, [])

  const addBatchResults = useCallback((batch: any[]) => {
    if (batch.length > 0) {
      batchQueue.current.push(batch)
      processBatchQueue()
    }
  }, [processBatchQueue])

  const completeScan = useCallback(() => {
    setIsScanning(false)
    setProgress(null)
  }, [])

  return (
    <ScanProgressContext.Provider
      value={{
        isScanning,
        progress,
        startScanning,
        updateProgress,
        completeScan,
        addBatchResults,
        directoryStructure
      }}
    >
      {children}
    </ScanProgressContext.Provider>
  )
}

export const useScanProgress = () => {
  const context = useContext(ScanProgressContext)
  if (context === undefined) {
    throw new Error('useScanProgress must be used within a ScanProgressProvider')
  }
  return context
}
