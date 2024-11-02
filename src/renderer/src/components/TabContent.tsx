import { useTabs } from '@renderer/context/TabContext'
import { useFileOperations } from '@renderer/hooks/useFileOperations'
import DirectoryView from './DirectoryView'
import { FileFilterOptions, FileFilter } from './FileFilter'
import FileGrid from './FileGrid'
import SearchBar from './SearchBar'

const TabContent: React.FC<{ tabId: string }> = ({ tabId }) => {
  const { state, dispatch } = useTabs()
  const { handleFileClick } = useFileOperations()
  const tab = state.tabs.find((t) => t.id === tabId)

  if (!tab) return null

  const handleDirectoryChange = (newPath: string[]) => {
    dispatch({
      type: 'UPDATE_TAB',
      payload: {
        ...tab,
        directoryPath: newPath
      }
    })
  }

  const handleSearch = async () => {
    if (tab.searchQuery.length > 0) {
      try {
        const results = await window.api.search(tab.directoryPath, tab.searchQuery)
        dispatch({
          type: 'UPDATE_TAB',
          payload: {
            ...tab,
            searchResults: results || []
          }
        })
      } catch (err) {
        window.api.sendMessage('Search error: ' + err)
        dispatch({
          type: 'UPDATE_TAB',
          payload: {
            ...tab,
            searchResults: []
          }
        })
      }
    } else {
      dispatch({
        type: 'UPDATE_TAB',
        payload: {
          ...tab,
          searchResults: []
        }
      })
    }
  }

  const updateSearchQuery = (query: string) => {
    dispatch({
      type: 'UPDATE_TAB',
      payload: {
        ...tab,
        searchQuery: query
      }
    })
  }

  const updateFileFilters = (filters: FileFilterOptions) => {
    dispatch({
      type: 'UPDATE_TAB',
      payload: {
        ...tab,
        fileFilters: filters
      }
    })
  }

  return (
    <div className="flex flex-col h-full">
      <div className="flex-shrink-0">
        <DirectoryView directoryPath={tab.directoryPath} onDirectoryClick={handleDirectoryChange} />
        <div className="flex items-center gap-4 mb-4">
          <SearchBar
            searchQuery={tab.searchQuery}
            setSearchQuery={updateSearchQuery}
            onSearch={handleSearch}
            setSearchResults={(results) => {
              dispatch({
                type: 'UPDATE_TAB',
                payload: {
                  ...tab,
                  searchResults: results
                }
              })
            }}
          />
          <FileFilter filters={tab.fileFilters} onFilterChange={updateFileFilters} />
        </div>
      </div>

      <div className="flex flex-grow justify-between items-start gap-2.5 max-w-7xl mx-auto h-full overflow-auto mb-24">
        <FileGrid
          directoryPath={tab.directoryPath}
          onDirectoryClick={handleDirectoryChange}
          onFileClick={(file) => handleFileClick(file, tab.directoryPath)}
          isSearching={tab.searchQuery.length > 0}
          searchResults={tab.searchResults}
          fileFilters={tab.fileFilters}
        />
      </div>
    </div>
  )
}

export default TabContent
