import { useTabs } from '@renderer/hooks/TabContext'
import TabContent from './TabContent'

const TabContainer = () => {
  const { state, dispatch } = useTabs()

  if (!state.activeTabId) {
    return (
      <div className="flex items-center justify-center h-full">
        <button
          className="px-4 py-2 bg-blue-500 text-white rounded"
          onClick={() => {
            dispatch({
              type: 'ADD_TAB',
              payload: {
                directoryPath: [],
                searchQuery: '',
                searchResults: [],
                fileFilters: {
                  all: true,
                  audio: false,
                  images: false,
                  text: false,
                  video: false,
                  directories: false
                },
                title: 'New Tab'
              }
            })
          }}
        >
          Create New Tab
        </button>
      </div>
    )
  }

  return <TabContent tabId={state.activeTabId} />
}

export default TabContainer
