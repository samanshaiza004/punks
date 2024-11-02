// src/hooks/useBatchLoading.ts
import { useState, useEffect } from 'react'
import { FileInfo } from '../types/FileInfo'
import { useToast } from '@renderer/context/ToastContext'

const BATCH_SIZE = 50
const INITIAL_LOAD_SIZE = 100

export function useBatchLoading(directoryPath: string[]) {
  const [files, setFiles] = useState<FileInfo[]>([])
  const [isLoading, setIsLoading] = useState(false)
  const [hasMore, setHasMore] = useState(true)
  const [offset, setOffset] = useState(0)
  const [totalFiles, setTotalFiles] = useState(0)

  const { showToast } = useToast()

  useEffect(() => {
    // Reset state when directory changes
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

  const sortFiles = (files: FileInfo[]): FileInfo[] => {
    return files.sort((a, b) => {
      if (a.isDirectory && !b.isDirectory) return -1
      if (!a.isDirectory && b.isDirectory) return 1
      return a.name.localeCompare(b.name)
    })
  }

  return {
    files,
    isLoading,
    hasMore,
    loadMoreFiles,
    totalFiles
  }
}
