import { useTabs } from '@renderer/context/TabContext'
import { useFileOperations } from '@renderer/hooks/useFileOperations'
import FileFilterButtons, { FileFilterOptions } from './FileFilters'
import FileGrid from './FileGrid'
import SearchBar from './SearchBar'
import FolderTree from './FolderTree'
import ResizablePanel from './ResizablePanel'

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
        const results = await window.api.search([tab.selectedFolder || tab.directoryPath[0]], tab.searchQuery)
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
        <div className="flex flex-col">
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
            <FileFilterButtons 
              filters={tab.fileFilters || {}} 
              onChange={updateFileFilters}
            />
          </div>
        </div>
      </div>

      <div className="flex flex-grow overflow-hidden">
        <ResizablePanel className="border-r border-gray-200 dark:border-gray-700">
          <FolderTree />
        </ResizablePanel>
        <div className="flex-1 overflow-auto">
          <FileGrid
            directoryPath={[tab.selectedFolder || tab.directoryPath[0]]}
            onDirectoryClick={handleDirectoryChange}
            onFileClick={handleFileClick}
            isSearching={tab.searchQuery.length > 0}
            searchResults={tab.searchResults}
            fileFilters={tab.fileFilters}
          />
        </div>
      </div>
    </div>
  )
}

export default TabContent
