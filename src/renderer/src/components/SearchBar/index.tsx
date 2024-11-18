import React, { useEffect, useRef } from 'react'
import { FileInfo } from '../../types/FileInfo'
import { KeyHandlerMap } from '@renderer/types/keybinds'
import { useKeyBindings } from '@renderer/keybinds/hooks'
import { useTheme } from '@renderer/context/ThemeContext'

interface SearchBarProps {
  searchQuery: string
  setSearchQuery: (query: string) => void
  onSearch: () => void
  setSearchResults: (results: FileInfo[]) => void
}

const SearchBar: React.FC<SearchBarProps> = ({
  searchQuery,
  setSearchQuery,
  onSearch,
  setSearchResults
}) => {
  const searchBarRef = useRef<HTMLInputElement>(null)
  const { isDarkMode } = useTheme()

  const handlers: KeyHandlerMap = {
    FOCUS_SEARCH: () => {
      searchBarRef.current?.focus()
    }
  }

  useKeyBindings(handlers)

  useEffect(() => {
    const delayDebounceFn = setTimeout(() => {
      if (searchQuery.length > 0) {
        onSearch()
      } else {
        setSearchResults([])
      }
    }, 300)
    return (): void => clearTimeout(delayDebounceFn)
  }, [searchQuery, onSearch, setSearchResults])

  return (
    <div className={`relative w-full ${isDarkMode ? 'text-gray-100' : 'text-gray-900'}`}>
      <div className="relative">
        <input
          id="search-bar"
          ref={searchBarRef}
          type="text"
          value={searchQuery}
          onChange={(e) => {
            try {
              setSearchQuery(e.target.value)
            } catch (err) {
              window.api.sendMessage('SearchBar.tsx: ' + err)
            }
          }}
          placeholder="Search files or directories"
          className={`
          w-full
          h-6
          py-1
          px-1
          rounded-sm
          border
          outline-none
          transition-colors
          placeholder:transition-colors
          text-sm
          ${
            isDarkMode
              ? 'bg-gray-800 border-gray-700 placeholder:text-gray-500 focus:border-gray-600'
              : 'bg-white border-gray-300 placeholder:text-gray-400 focus:border-gray-400'
          }
          ${isDarkMode ? 'focus:ring-2 focus:ring-gray-700' : 'focus:ring-2 focus:ring-gray-200'}
        `}
        />
        {searchQuery && (
          <button
            onClick={() => setSearchQuery('')}
            className={`
            absolute
            right-3
            top-1/2
            transform
            -translate-y-1/2
            w-6
            h-6
            flex
            items-center
            justify-center
            rounded-full
            transition-colors
            ${
              isDarkMode
                ? 'text-gray-400 hover:text-gray-300 hover:bg-gray-700'
                : 'text-gray-500 hover:text-gray-600 hover:bg-gray-100'
            }
          `}
          >
            ×
          </button>
        )}
      </div>
    </div>
  )
}

export default SearchBar
