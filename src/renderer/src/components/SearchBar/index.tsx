// src/components/SearchBar/index.tsx

import React, { useEffect, useRef } from 'react'
import { FileInfo } from '../../types/FileInfo'
import { useHotkeys } from 'react-hotkeys-hook'
// const { ipcMain, ipcRenderer } = window.require('electron')

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
  useHotkeys('ctrl+l', () => {
    searchBarRef.current?.focus()
  })
  useEffect(() => {
    const delayDebounceFn = setTimeout(() => {
      if (searchQuery.length > 0) {
        onSearch()
      } else {
        setSearchResults([])
      }
    }, 300)

    return () => clearTimeout(delayDebounceFn)
  }, [searchQuery, onSearch, setSearchResults])
  useEffect(() => {}, [])
  return (
    <div>
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
      />
    </div>
  )
}

export default SearchBar
