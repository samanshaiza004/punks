import React from 'react'
import { ArrowRight } from '@phosphor-icons/react'
import { KeyHandlerMap } from '@renderer/types/keybinds'
import { useKeyBindings } from '@renderer/keybinds/hooks'
import { useTheme } from '@renderer/context/ThemeContext'

interface DirectoryBreadcrumbsProps {
  directoryPath: string[]
  onDirectoryClick: (path: string[]) => void
}

const DirectoryBreadcrumbs: React.FC<DirectoryBreadcrumbsProps> = ({ directoryPath, onDirectoryClick }) => {
  const { isDarkMode } = useTheme()

  const handleClick = (index: number) => {
    onDirectoryClick(directoryPath.slice(0, index + 1))
  }

  const goBack = () => {
    if (directoryPath.length > 1) {
      onDirectoryClick(directoryPath.slice(0, -1))
    }
  }

  const handlers: KeyHandlerMap = {
    GO_BACK: () => {
      goBack()
    }
  }

  useKeyBindings(handlers)

  const containerClasses = `
    flex items-center space-x-1 px-2 py-1.5 
    overflow-x-auto scrollbar-thin scrollbar-thumb-gray-400 scrollbar-track-transparent
    ${isDarkMode
      ? 'bg-gray-700 text-gray-200'
      : 'bg-gray-100 text-gray-800'}
    border-b
  `

  const breadcrumbClasses = `
    text-sm px-1 py-0.5
    flex items-center
    rounded
    hover:bg-opacity-80
    transition-colors duration-100
    focus:outline-none focus:ring-1 focus:ring-blue-500
    
  `

  const chevronClasses = `
    w-3 h-3 mx-0.5 flex-shrink-0
    ${isDarkMode ? 'text-gray-500' : 'text-gray-400'}
  `

  return (
    <div className={containerClasses.trim()}>
      {directoryPath.map((directory, index) => (
        <React.Fragment key={index}>
          {index > 0 && <ArrowRight className={chevronClasses} />}
          <button
            onClick={() => handleClick(index)}
            className={breadcrumbClasses.trim()}
            title={directoryPath.slice(0, index + 1).join('/')}
          >
            {directory}
          </button>
        </React.Fragment>
      ))}
    </div>
  )
}

export default DirectoryBreadcrumbs
