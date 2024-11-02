import { X, Plus, List } from '@phosphor-icons/react'
import { useTabs } from '@renderer/context/TabContext'
import { useTheme } from '@renderer/context/ThemeContext'

const TabBar = ({ lastSelectedDirectory }) => {
  const { state, dispatch } = useTabs()
  const { isDarkMode } = useTheme()

  return (
    <div
      className={`
      flex items-center 
      border-b
      ${isDarkMode ? 'border-gray-700' : 'border-gray-200'}
      mb-2 relative
    `}
    >
      <div className="flex-1 flex items-center overflow-x-auto hide-scrollbar">
        {state.tabs.map((tab) => (
          <div
            key={tab.id}
            className={`
              group
              flex items-center
              min-w-[140px]
              max-w-[200px]
              px-3 py-2
              mr-1
              rounded-t-md
              transition-all
              duration-200
              select-none
              cursor-pointer
              focus:outline-none
              focus:ring-2
              focus:ring-blue-400
              ${
                state.activeTabId === tab.id
                  ? isDarkMode
                    ? 'bg-gray-800 text-white border-t-2 border-x-2 border-blue-500'
                    : 'bg-white text-gray-800 border-t-2 border-x-2 border-blue-500'
                  : isDarkMode
                    ? 'bg-gray-900 text-gray-400 hover:bg-gray-800 hover:text-gray-200'
                    : 'bg-gray-100 text-gray-600 hover:bg-gray-200'
              }
            `}
            onClick={() => dispatch({ type: 'SET_ACTIVE_TAB', payload: tab.id })}
          >
            <span className="flex-1 truncate text-sm">{tab.title}</span>
            <button
              className={`
                ml-2
                p-1
                rounded-full
                opacity-0
                group-hover:opacity-100
                transition-opacity
                duration-200
                ${
                  state.activeTabId === tab.id
                    ? isDarkMode
                      ? 'hover:bg-gray-700 text-gray-300 hover:text-white'
                      : 'hover:bg-gray-200 text-gray-500 hover:text-gray-700'
                    : isDarkMode
                      ? 'hover:bg-gray-700 text-gray-400 hover:text-white'
                      : 'hover:bg-gray-300 text-gray-500 hover:text-gray-700'
                }
              `}
              onClick={(e) => {
                e.stopPropagation()
                dispatch({ type: 'REMOVE_TAB', payload: tab.id })
              }}
              title="Close tab"
            >
              <X size={14} />
            </button>
          </div>
        ))}
      </div>

      <div
        className={`
        flex items-center
        px-2
        border-l
        ${isDarkMode ? 'border-gray-700' : 'border-gray-200'}
      `}
      >
        <button
          className={`
            p-1.5
            rounded-md
            transition-colors
            duration-200
            focus:outline-none
            focus:ring-2
            focus:ring-blue-400
            ${
              isDarkMode
                ? 'hover:bg-gray-800 text-gray-400 hover:text-gray-200'
                : 'hover:bg-gray-200 text-gray-600 hover:text-gray-800'
            }
          `}
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
          title="New tab"
        >
          <Plus size={18} />
        </button>

        <button
          className={`
            p-1.5
            rounded-md
            transition-colors
            duration-200
            focus:outline-none
            focus:ring-2
            focus:ring-blue-400
            ${
              isDarkMode
                ? 'hover:bg-gray-800 text-gray-400 hover:text-gray-200'
                : 'hover:bg-gray-200 text-gray-600 hover:text-gray-800'
            }
          `}
          title="Tab options"
        >
          <List size={18} />
        </button>
      </div>
    </div>
  )
}

export default TabBar
