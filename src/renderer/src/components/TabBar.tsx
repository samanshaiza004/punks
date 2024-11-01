import { useTabs } from '@renderer/hooks/TabContext'

interface TabBarProps {
  lastSelectedDirectory: string[]
}

// components/TabBar.tsx
const TabBar: React.FC<TabBarProps> = ({ lastSelectedDirectory }) => {
  const { state, dispatch } = useTabs()

  return (
    <div className="flex items-center border-b">
      {state.tabs.map((tab) => (
        <div
          key={tab.id}
          className={`px-2 py-2 cursor-pointer ${
            state.activeTabId === tab.id ? 'bg-blue-500 text-white' : ''
          }`}
          onClick={() => dispatch({ type: 'SET_ACTIVE_TAB', payload: tab.id })}
        >
          <span>{tab.title}</span>
          <button
            className={`ml-2 px-1 rounded-full cursor-pointer ${
              state.activeTabId === tab.id ? 'hover:bg-blue-700 text-white' : 'hover:bg-slate-200'
            }`}
            onClick={(e) => {
              e.stopPropagation()
              dispatch({ type: 'REMOVE_TAB', payload: tab.id })
            }}
          >
            ×
          </button>
        </div>
      ))}
      <button
        className="px-4 py-2 hover:bg-slate-200"
        onClick={() => {
          dispatch({
            type: 'ADD_TAB',
            payload: {
              directoryPath: lastSelectedDirectory,
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
              title: `New Tab ${state.tabs.length + 1}`
            }
          })
        }}
      >
        +
      </button>
    </div>
  )
}

export default TabBar
