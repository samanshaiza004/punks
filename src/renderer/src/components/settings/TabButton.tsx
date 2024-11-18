import React from 'react'
import { useTheme } from '@renderer/context/ThemeContext'

interface TabButtonProps {
  isActive: boolean
  onClick: () => void
  icon: React.ReactNode
  label: string
}

const TabButton: React.FC<TabButtonProps> = ({ isActive, onClick, icon, label }) => {
  const { isDarkMode } = useTheme()

  return (
    <button
      onClick={onClick}
      className={`
        w-full px-4 py-3 text-left flex items-center gap-3 transition-colors
        ${isActive
          ? isDarkMode
            ? 'bg-gray-700 text-white'
            : 'bg-gray-200 text-gray-900'
          : isDarkMode
          ? 'text-gray-300 hover:bg-gray-700/50'
          : 'text-gray-700 hover:bg-gray-100'
        }
      `}
    >
      <span className="text-lg">{icon}</span>
      <span className="text-sm font-medium">{label}</span>
    </button>
  )
}

export default TabButton
