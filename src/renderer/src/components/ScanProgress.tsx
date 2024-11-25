import React from 'react'
import * as Progress from '@radix-ui/react-progress'
import { useTheme } from '@renderer/context/ThemeContext'

interface ScanProgressProps {
  total: number
  processed: number
  percentComplete: number
}

const ScanProgress: React.FC<ScanProgressProps> = ({ total, processed, percentComplete }) => {
  const { isDarkMode } = useTheme()

  return (
    <div
      className={`
        fixed bottom-4 right-4
        p-3 rounded-lg shadow-lg
        ${isDarkMode ? 'bg-gray-800' : 'bg-white'}
        border ${isDarkMode ? 'border-gray-700' : 'border-gray-200'}
        max-w-xs w-64
        transition-all duration-200 ease-in-out
        z-50
      `}
    >
      <div className="flex items-center justify-between mb-2">
        <span className={`text-sm font-medium ${isDarkMode ? 'text-gray-200' : 'text-gray-800'}`}>
          Scanning Files...
        </span>
        <span className={`text-xs ${isDarkMode ? 'text-gray-400' : 'text-gray-600'}`}>
          {Math.round(percentComplete)}%
        </span>
      </div>
      
      <Progress.Root
        className={`relative overflow-hidden bg-gray-200 rounded-full h-1.5 ${
          isDarkMode ? 'bg-gray-700' : 'bg-gray-200'
        }`}
        value={percentComplete}
      >
        <Progress.Indicator
          className="bg-blue-500 w-full h-full transition-transform duration-[660ms] ease-[cubic-bezier(0.65, 0, 0.35, 1)]"
          style={{ transform: `translateX(-${100 - percentComplete}%)` }}
        />
      </Progress.Root>

      <div className={`text-xs mt-1 ${isDarkMode ? 'text-gray-400' : 'text-gray-600'}`}>
        {processed.toLocaleString()} / {total.toLocaleString()} files
      </div>
    </div>
  )
}

export default ScanProgress
