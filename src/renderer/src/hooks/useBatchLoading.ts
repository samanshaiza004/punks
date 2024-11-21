// src/hooks/useBatchLoading.ts
import { useState, useEffect } from 'react'
import { FileNode, FileSystemNode } from '../../../types'
import { useToast } from '@renderer/context/ToastContext'

const BATCH_SIZE = 50
const INITIAL_LOAD_SIZE = 100

export function useBatchLoading(directoryPath: string[]): {
  files: FileNode[]
  isLoading: boolean
  hasMore: boolean
  loadMoreFiles: () => void
  totalFiles: number
} {
  const [files, setFiles] = useState<FileNode[]>([])
  const [isLoading, setIsLoading] = useState(false)
  const [hasMore, setHasMore] = useState(true)
  const [offset, setOffset] = useState(0)
  const [totalFiles, setTotalFiles] = useState(0)

  const { showToast } = useToast()

  useEffect(() => {
    setFiles([])
    setOffset(0)
    setHasMore(true)
    loadInitialBatch()
  }, [directoryPath])

  const loadInitialBatch = async () => {
    setIsLoading(true)
    try {
      const result = await window.api.readDir(directoryPath, 0, INITIAL_LOAD_SIZE)
      setFiles(sortFiles(result.files))
      setTotalFiles(result.total)
      setOffset(result.files.length)
      setHasMore(result.hasMore)
    } catch (err) {
      window.api.sendMessage('Error loading initial batch: ' + err)
    }
    setIsLoading(false)
  }

  const loadMoreFiles = async () => {
    if (isLoading || !hasMore) return

    setIsLoading(true)
    try {
      const result = await window.api.readDir(directoryPath, offset, BATCH_SIZE)
      setFiles((prev) => [...prev, ...sortFiles(result.files)])
      setOffset((prev) => prev + result.files.length)
      setHasMore(result.hasMore)
    } catch (err) {
      showToast('Error loading more files: ' + err, 'error')
    }
    setIsLoading(false)
  }

  const sortFiles = (files: FileSystemNode[]): FileNode[] => {
    return files
      .sort((a, b) => {
        const isADirectory = a.type === 'directory'
        const isBDirectory = b.type === 'directory'
        
        if (isADirectory && !isBDirectory) return -1
        if (!isADirectory && isBDirectory) return 1
        return a.name.localeCompare(b.name)
      })
      .filter((file): file is FileNode => file.type === 'file')
  }

  return {
    files,
    isLoading,
    hasMore,
    loadMoreFiles,
    totalFiles
  }
}
