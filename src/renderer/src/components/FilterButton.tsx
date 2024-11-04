// components/FilterButton.tsx
import React from 'react'
import { useTheme } from '@renderer/context/ThemeContext'
import { FileFilterOptions } from './FileFilter'

interface FilterButtonProps {
  type: keyof FileFilterOptions
  label: string
  isActive: boolean
  onToggle: (type: keyof FileFilterOptions) => void
}

export const FilterButton: React.FC<FilterButtonProps> = ({ type, label, isActive, onToggle }) => {
  const { isDarkMode } = useTheme()

  return (
    <button
      onClick={() => onToggle(type)}
      className={`px-3 py-1 rounded-sm text-sm font-medium transition-colors
        ${
          isActive
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
}
