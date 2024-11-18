import { useTabs } from '@renderer/context/TabContext'
import { useFileOperations } from '@renderer/hooks/useFileOperations'
import DirectoryView from './DirectoryView'
import FileFilterButtons, { FileFilterOptions } from './FileFilters'
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

  const handleSearch = async (): Promise<void> => {
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

  const updateSearchQuery = (query: string): void => {
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
      <div className="flex-shrink-0 p-1">
        <DirectoryView directoryPath={tab.directoryPath} onDirectoryClick={handleDirectoryChange} />
        <div className="flex flex-col sm:flex-row items-stretch sm:items-center gap-1 mb-1">
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
          <FileFilterButtons filters={tab.fileFilters} onFilterChange={updateFileFilters} />
        </div>
      </div>

      <div className="flex flex-grow overflow-auto mb-24">
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
