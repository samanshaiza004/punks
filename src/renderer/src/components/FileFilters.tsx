import React from 'react'
import * as DropdownMenu from '@radix-ui/react-dropdown-menu'
import { useTheme } from '../../../renderer/src/context/ThemeContext'
import { Button } from './Button'
import { Funnel, Check } from '@phosphor-icons/react'

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

  const activeFiltersCount = Object.entries(filters)
    .filter(([key, value]) => key !== 'all' && value)
    .length;

  return (
    <DropdownMenu.Root>
      <DropdownMenu.Trigger asChild>
        <Button className="gap-2">
          <Funnel className="h-4 w-4" />
          <span>Filter</span>
          {(activeFiltersCount > 0 || filters.all) && (
            <span className="ml-1 rounded-full bg-white text-blue-500 px-1.5 py-0.5 text-xs font-medium">
              {filters.all ? 'All' : activeFiltersCount}
            </span>
          )}
        </Button>
      </DropdownMenu.Trigger>

      <DropdownMenu.Portal>
        <DropdownMenu.Content
          className={`min-w-[220px] rounded-md p-1 shadow-lg ${
            isDarkMode 
              ? 'bg-gray-800 text-gray-100' 
              : 'bg-white text-gray-900'
          }`}
          sideOffset={5}
        >
          {Object.keys(filters).map((filterKey) => (
            <DropdownMenu.CheckboxItem
              key={filterKey}
              checked={filters[filterKey as keyof FileFilterOptions]}
              onCheckedChange={() => handleToggle(filterKey as keyof FileFilterOptions)}
              className={`
                relative flex items-center px-2 py-2 text-sm outline-none select-none rounded-sm
                ${isDarkMode 
                  ? 'hover:bg-gray-700 focus:bg-gray-700' 
                  : 'hover:bg-gray-100 focus:bg-gray-100'
                }
              `}
            >
              <div className="w-4 h-4 mr-2">
                {filters[filterKey as keyof FileFilterOptions] && (
                  <Check weight="bold" />
                )}
              </div>
              {filterKey[0].toUpperCase() + filterKey.slice(1)}
            </DropdownMenu.CheckboxItem>
          ))}
        </DropdownMenu.Content>
      </DropdownMenu.Portal>
    </DropdownMenu.Root>
  )
}

export default FileFilterButtons
