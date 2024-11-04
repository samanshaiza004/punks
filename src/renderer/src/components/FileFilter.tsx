// components/FileFilter.tsx
import React from 'react'
import { FilterButton } from './FilterButton'

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

  return (
    <div className="flex gap-1 items-center">
      <span className="text-sm">Show:</span>
      <div className="flex gap-1">
        {Object.keys(filters).map((filterKey) => (
          <FilterButton
            key={filterKey}
            type={filterKey as keyof FileFilterOptions}
            label={filterKey[0].toUpperCase() + filterKey.slice(1)}
            isActive={filters[filterKey as keyof FileFilterOptions]}
            onToggle={handleToggle}
          />
        ))}
      </div>
    </div>
  )
}
