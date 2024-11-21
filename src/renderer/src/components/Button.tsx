/* eslint-disable react/prop-types */
import React from 'react'
import { useTheme } from '@renderer/context/ThemeContext'

type ButtonProps = React.ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: 'primary' | 'secondary'
}

export const Button = React.forwardRef<HTMLButtonElement, ButtonProps>(
  ({ children, variant = 'primary', className, ...props }, ref) => {
    const { isDarkMode } = useTheme()

    const baseClasses =
      'px-2 py-1 text-sm rounded-sm focus:outline-none focus:ring-2 focus:ring-offset-2'
    const lightClasses =
      variant === 'primary'
        ? 'bg-blue-500 text-white hover:bg-blue-600 focus:ring-blue-500'
        : 'bg-gray-200 text-gray-800 hover:bg-gray-300 focus:ring-gray-500'
    const darkClasses =
      variant === 'primary'
        ? 'bg-blue-700 text-white hover:bg-blue-800 focus:ring-blue-700'
        : 'bg-gray-700 text-white hover:bg-gray-600 focus:ring-gray-700'

    const themeClasses = isDarkMode ? darkClasses : lightClasses

    return (
      <button
        ref={ref}
        className={`${baseClasses} ${themeClasses} ${className}`}
        {...props}
      >
        {children}
      </button>
    )
  }
)
