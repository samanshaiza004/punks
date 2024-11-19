import React from 'react'
import { useTheme } from '@renderer/context/ThemeContext'

interface ScanProgressProps {
  total: number
  processed: number
  percentComplete: number
}

const ScanProgress: React.FC<ScanProgressProps> = ({ total, processed, percentComplete }) => {
  const { isDarkMode } = useTheme()

  return (
    <div className="h-full w-full flex items-center justify-center">
      <div
        className={`
          text-center
          p-8
          rounded-lg
          max-w-md
          w-full
        `}
      >
        <div className="mb-4">
          <div
            className={`
              h-2
              w-full
              rounded-full
              ${isDarkMode ? 'bg-gray-700' : 'bg-gray-200'}
              overflow-hidden
            `}
          >
            <div
              className="h-full bg-blue-500 transition-all duration-300"
              style={{ width: `${percentComplete}%` }}
            />
          </div>
        </div>
        <div className={`text-sm ${isDarkMode ? 'text-gray-400' : 'text-gray-600'}`}>
          <span className="font-medium">
            {processed.toLocaleString()} / {total.toLocaleString()}
          </span>{' '}
          files processed
        </div>
        <div className={`text-xs mt-1 ${isDarkMode ? 'text-gray-500' : 'text-gray-500'}`}>
          {percentComplete}% complete
        </div>
      </div>
    </div>
  )
}

export default ScanProgress
