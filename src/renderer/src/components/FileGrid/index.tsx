// src/components/FileGrid.tsx
import React, { useCallback, useEffect, useState, useRef } from 'react'
import { FileItem } from '../FileItem'
import { FileInfo } from '@renderer/types/FileInfo'
import { useTheme } from '@renderer/context/ThemeContext'
import { useBatchLoading } from '@renderer/hooks/useBatchLoading'

interface FileGridProps {
  directoryPath: string[]
  onDirectoryClick: (path: string[]) => void
  onFileClick: (file: FileInfo) => void
  isSearching?: boolean
  searchResults?: FileInfo[]
}

const FileGrid: React.FC<FileGridProps> = ({
  directoryPath,
  onDirectoryClick,
  onFileClick,
  isSearching = false,
  searchResults = []
}) => {
  const { files, isLoading, hasMore, loadMoreFiles, totalFiles } = useBatchLoading(directoryPath)
  const [selectedIndex, setSelectedIndex] = useState(0)
  const { isDarkMode } = useTheme()
  const observerRef = useRef<IntersectionObserver | null>(null)
  const loadingTriggerRef = useRef<HTMLDivElement>(null)

  // Set up intersection observer for infinite scrolling
  useEffect(() => {
    if (isSearching) return // Don't use infinite scroll for search results

    const options = {
      root: null,
      rootMargin: '100px',
      threshold: 0.1
    }

    observerRef.current = new IntersectionObserver(([entry]) => {
      if (entry.isIntersecting && hasMore && !isLoading) {
        loadMoreFiles()
      }
    }, options)

    if (loadingTriggerRef.current) {
      observerRef.current.observe(loadingTriggerRef.current)
    }

    return () => {
      if (observerRef.current) {
        observerRef.current.disconnect()
      }
    }
  }, [hasMore, isLoading, isSearching])

  const displayedFiles = isSearching ? searchResults : files

  if (displayedFiles.length === 0 && !isLoading) {
    return (
      <div
        className={`col-span-full flex flex-col items-center justify-center p-8 rounded-lg
        ${isDarkMode ? 'bg-gray-700 text-gray-200' : 'bg-gray-50 text-gray-600'}`}
      >
        {isSearching ? (
          <>
            <p className="text-xl font-semibold mb-2">No search results found</p>
            <p className="text-sm">Try adjusting your search terms</p>
          </>
        ) : (
          <>
            <p className="text-xl font-semibold mb-2">This folder is empty</p>
            <p className="text-sm">Add some files to get started</p>
          </>
        )}
      </div>
    )
  }

  return (
    <div className="relative w-full h-full overflow-auto">
      <div
        className={`grid grid-cols-4 xs:grid-cols-2 gap-2 p-4 auto-rows-fr
        ${isDarkMode ? 'bg-gray-900 text-gray-200' : 'bg-white text-gray-800'}`}
      >
        {(isSearching ? searchResults : files).map((file, index) => (
          <FileItem
            key={`${file.name}-${index}`}
            onClick={() => {
              setSelectedIndex(index)
              if (file.isDirectory) {
                onDirectoryClick([...directoryPath, file.name])
              } else {
                onFileClick(file)
              }
            }}
            fileName={file.name}
            isDirectory={file.isDirectory}
            location={window.api.renderPath([...directoryPath, file.name])}
            isSelected={index === selectedIndex}
            isDarkMode={isDarkMode}
          />
        ))}

        {!isSearching && hasMore && (
          <div ref={loadingTriggerRef} className="col-span-full flex justify-center p-4">
            {isLoading ? (
              <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-gray-500" />
            ) : (
              <p className="text-sm text-gray-500">
                Showing {files.length} of {totalFiles} items
              </p>
            )}
          </div>
        )}
      </div>
    </div>
  )
}

export default FileGrid
