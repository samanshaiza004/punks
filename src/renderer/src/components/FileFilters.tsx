import React from 'react'
import * as ToggleGroup from '@radix-ui/react-toggle-group'
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

const FileFilterButtons: React.FC<FileFilterProps> = ({ filters, onFilterChange }) => {
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
      onFilterChange({
        ...filters,
        all: false,
        [filterKey]: !filters[filterKey]
      })
    }
  }

  const buttonClass = (isActive: boolean) => `
    px-2 py-1 text-sm font-medium
    ${
      isDarkMode
        ? isActive
          ? 'bg-blue-600 text-white'
          : 'bg-gray-700 text-gray-300 hover:bg-gray-600'
        : isActive
        ? 'bg-blue-500 text-white'
        : 'bg-gray-200 text-gray-700 hover:bg-gray-300'
    }
    focus:outline-none focus:ring-2 focus:ring-offset-2
    ${isDarkMode ? 'focus:ring-blue-500' : 'focus:ring-blue-400'}
    transition-colors
  `

  return (
    <div className="flex gap-1 items-center">
      <span className="text-sm">Show:</span>
      <ToggleGroup.Root
        type="multiple"
        className="inline-flex gap-1"
        aria-label="File filters"
      >
        {Object.keys(filters).map((filterKey) => (
          <ToggleGroup.Item
            key={filterKey}
            value={filterKey}
            aria-pressed={filters[filterKey as keyof FileFilterOptions]}
            onClick={() => handleToggle(filterKey as keyof FileFilterOptions)}
            className={buttonClass(filters[filterKey as keyof FileFilterOptions])}
          >
            {filterKey[0].toUpperCase() + filterKey.slice(1)}
          </ToggleGroup.Item>
        ))}
      </ToggleGroup.Root>
    </div>
  )
}

export default FileFilterButtons
