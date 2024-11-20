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
    <div className="h-full w-full flex items-center justify-center">
      <div
        className={`
          text-center
          p-8
          rounded-lg
          max-w-md
          w-full
          ${isDarkMode ? 'bg-gray-800/50' : 'bg-gray-50'}
        `}
      >
        <h3 className={`text-lg font-semibold mb-4 ${isDarkMode ? 'text-gray-200' : 'text-gray-800'}`}>
          Scanning Directory
        </h3>
        
        <Progress.Root
          className={`relative overflow-hidden bg-gray-200 rounded-full w-full h-2 ${
            isDarkMode ? 'bg-gray-700' : 'bg-gray-200'
          }`}
          style={{
            transform: 'translateZ(0)'
          }}
          value={percentComplete}
        >
          <Progress.Indicator
            className="bg-blue-500 w-full h-full transition-transform duration-[660ms] ease-[cubic-bezier(0.65, 0, 0.35, 1)]"
            style={{ transform: `translateX(-${100 - percentComplete}%)` }}
          />
        </Progress.Root>

        <div className={`text-sm mt-4 ${isDarkMode ? 'text-gray-400' : 'text-gray-600'}`}>
          <span className="font-medium">
            {processed.toLocaleString()} / {total.toLocaleString()}
          </span>{' '}
          files processed
        </div>
        
        <div className={`text-xs mt-1 ${isDarkMode ? 'text-gray-500' : 'text-gray-500'}`}>
          {Math.round(percentComplete)}% complete
        </div>
      </div>
    </div>
  )
}

export default ScanProgress
