import { ButtonHTMLAttributes } from 'react'

type FileAddressItemProps = {
  fileName: string
  isDarkMode: boolean
} & ButtonHTMLAttributes<HTMLButtonElement>

export function FileAddressItem({ fileName, isDarkMode, ...props }: FileAddressItemProps) {
  return (
    <button
      className={`
        h-8 
        px-2 
        flex 
        items-center 
        rounded-sm
        whitespace-nowrap 
        overflow-hidden 
        font-medium 
        cursor-pointer 
        transition-colors
        focus:outline-none 
        focus:ring-2
        ${
          isDarkMode
            ? 'text-gray-200 hover:bg-gray-700 focus:ring-gray-600'
            : 'text-gray-800 hover:bg-gray-200 focus:ring-gray-300'
        }
      `}
      {...props}
    >
      {fileName}
    </button>
  )
}
