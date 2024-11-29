/* eslint-disable @typescript-eslint/explicit-function-return-type */
import { ButtonHTMLAttributes } from 'react'

type BreadcrumbItemProps = {
  label: string
  isDarkMode: boolean
  isLast?: boolean
} & ButtonHTMLAttributes<HTMLButtonElement>

export function BreadcrumbItem({ label, isDarkMode, isLast, ...props }: BreadcrumbItemProps) {
  return (
    <button
      className={`
        text-sm px-1 py-0.5
        flex items-center
        rounded
        cursor-pointer
        transition-colors duration-100
        focus:outline-none focus:ring-1 focus:ring-blue-500
        ${
          isDarkMode
            ? `${isLast ? 'text-gray-300' : 'text-gray-400'} `
            : `${isLast ? 'text-gray-700' : 'text-gray-600'} `
        }
      `}
      {...props}
    >
      {label}
    </button>
  )
}
