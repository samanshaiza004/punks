// components/FileFilter.tsx
import React from 'react'
import { useTheme } from '@renderer/context/ThemeContext'

export interface FileFilterOptions {
  all: boolean
  audio: boolean
  images: boolean
  text: boolean
  video: boolean
  directories: boolean
}

interface FileFilterProps {
  filters: FileFilterOptions
  onFilterChange: (newFilters: FileFilterOptions) => void
}

export const FileFilter: React.FC<FileFilterProps> = ({ filters, onFilterChange }) => {
  const { isDarkMode } = useTheme()

  const handleToggle = (filterKey: keyof FileFilterOptions) => {
    if (filterKey === 'all') {
      const newState = !filters.all
      onFilterChange({
        all: newState,
        audio: false,
        images: false,
        text: false,
        video: false,
        directories: false
      })
    } else {
      // When toggling other filters, turn off 'all'
      onFilterChange({
        ...filters,
        all: false,
        [filterKey]: !filters[filterKey]
      })
    }
  }

  const FilterButton = ({ type, label }: { type: keyof FileFilterOptions; label: string }) => (
    <button
      onClick={() => handleToggle(type)}
      className={`px-3 py-1 rounded-sm text-sm font-medium transition-colors
        ${
          filters[type]
            ? `${isDarkMode ? 'bg-blue-600 text-white' : 'bg-blue-500 text-white'}`
            : `${
                isDarkMode
                  ? 'bg-gray-700 text-gray-300 hover:bg-gray-600'
                  : 'bg-gray-200 text-gray-700 hover:bg-gray-300'
              }`
        }`}
    >
      {label}
    </button>
  )

  return (
    <div className="flex gap-2 items-center">
      <span className={`text-sm ${isDarkMode ? 'text-gray-300' : 'text-gray-700'}`}>Show:</span>
      <div className="flex gap-2">
        <FilterButton type="all" label="All" />
        <FilterButton type="audio" label="Audio" />
        <FilterButton type="images" label="Images" />
        <FilterButton type="video" label="Video" />
        <FilterButton type="text" label="Text" />
        <FilterButton type="directories" label="Folders" />
      </div>
    </div>
  )
}
