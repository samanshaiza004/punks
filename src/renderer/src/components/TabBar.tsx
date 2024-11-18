import { useRef, useState, useEffect, useCallback } from 'react'
import { X, Plus, List } from '@phosphor-icons/react'
import { useTabs } from '@renderer/context/TabContext'
import { useTheme } from '@renderer/context/ThemeContext'
import { useKeyBindings } from '@renderer/keybinds/hooks'
import { KeyHandlerMap } from '@renderer/types/keybinds'

const TabBar = ({ lastSelectedDirectory }) => {
  const { state, dispatch } = useTabs()
  const { isDarkMode } = useTheme()
  const scrollContainerRef = useRef<HTMLDivElement>(null)
  const [showLeftIndicator, setShowLeftIndicator] = useState(false)
  const [showRightIndicator, setShowRightIndicator] = useState(false)

  // Check if we need to show scroll indicators
  const checkScroll = () => {
    const el = scrollContainerRef.current
    if (el) {
      setShowLeftIndicator(el.scrollLeft > 0)
      setShowRightIndicator(el.scrollLeft < el.scrollWidth - el.clientWidth)
    }
  }

  // Add scroll event listener
  useEffect(() => {
    const el = scrollContainerRef.current
    if (el) {
      el.addEventListener('scroll', checkScroll)
      // Initial check
      checkScroll()
      // Check on window resize
      window.addEventListener('resize', checkScroll)
    }
    return () => {
      el?.removeEventListener('scroll', checkScroll)
      window.removeEventListener('resize', checkScroll)
    }
  }, [])

  // Scroll functions
  const scrollLeft = () => {
    const el = scrollContainerRef.current
    if (el) {
      el.scrollBy({ left: -200, behavior: 'smooth' })
    }
  }

  const scrollRight = () => {
    const el = scrollContainerRef.current
    if (el) {
      el.scrollBy({ left: 200, behavior: 'smooth' })
    }
  }

  const handleCloseTab = useCallback((tabId: string) => {
    dispatch({ type: 'REMOVE_TAB', payload: tabId })
  }, [dispatch])

  const handleNewTab = useCallback(() => {
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
        title: 'New Tab'
      }
    })
  }, [dispatch, lastSelectedDirectory])

  const handleNextTab = useCallback(() => {
    if (state.tabs.length <= 1) return

    const currentIndex = state.tabs.findIndex(tab => tab.id === state.activeTabId)
    const nextIndex = (currentIndex + 1) % state.tabs.length
    dispatch({ type: 'SET_ACTIVE_TAB', payload: state.tabs[nextIndex].id })
  }, [dispatch, state.tabs, state.activeTabId])

  const handlers: KeyHandlerMap = {
    CLOSE_TAB: () => {
      if (state.activeTabId) {
        handleCloseTab(state.activeTabId)
      }
    },
    NEW_TAB: handleNewTab,
    NEXT_TAB: handleNextTab
  }

  useKeyBindings(handlers)

  return (
    <div className="relative flex items-center border-b mb-2">
      {showLeftIndicator && (
        <div
          onClick={scrollLeft}
          className={`absolute left-0 z-10 w-8 h-full flex items-center justify-center cursor-pointer ${
            isDarkMode
              ? 'bg-gradient-to-r from-gray-900 to-transparent'
              : 'bg-gradient-to-r from-white to-transparent'
          }`}
        >
          <div
            className={`p-1 rounded-full ${
              isDarkMode ? 'hover:bg-gray-800 text-gray-400' : 'hover:bg-gray-200 text-gray-600'
            }`}
          >
            ◀
          </div>
        </div>
      )}

      {/* Tabs container */}
      <div
        ref={scrollContainerRef}
        className="flex-1 flex items-center overflow-x-auto show-scrollbar-on-hover"
      >
        {state.tabs.map((tab) => (
          <div
            key={tab.id}
            className={`
              group
              flex items-center
              min-w-[120px]
              max-w-[180px]
              px-2 py-1
              mr-1
              rounded-t-sm
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

      {/* Right scroll indicator */}
      {showRightIndicator && (
        <div
          onClick={scrollRight}
          className={`absolute right-12 z-10 w-8 h-full flex items-center justify-center cursor-pointer ${
            isDarkMode
              ? 'bg-gradient-to-l from-gray-900 to-transparent'
              : 'bg-gradient-to-l from-white to-transparent'
          }`}
        >
          <div
            className={`p-1 rounded-full ${
              isDarkMode ? 'hover:bg-gray-800 text-gray-400' : 'hover:bg-gray-200 text-gray-600'
            }`}
          >
            ▶
          </div>
        </div>
      )}

      {/* Actions section */}
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
