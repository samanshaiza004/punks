
import { v4 as uuidv4 } from 'uuid'
import { createContext, useContext, useReducer } from 'react'
import { FileNode } from '../../../types/index'
import { FileFilterOptions } from '@renderer/components/FileFilters'

interface TabState {
  tabs: Tab[]
  activeTabId: string | null
  layout: 'tabs' | 'tiles'
  tileConfig: TileConfig[]
}

interface TileConfig {
  id: string
  tabIds: string[]
  split: 'hor' | 'vert'
  size: number
}

type TabAction =
  | { type: 'ADD_TAB'; payload: Omit<Tab, 'id'> }
  | { type: 'REMOVE_TAB'; payload: string }
  | { type: 'UPDATE_TAB'; payload: Tab }
  | { type: 'SET_ACTIVE_TAB'; payload: string }
  | { type: 'SET_LAYOUT'; payload: 'tabs' | 'tiles' }
  | { type: 'UPDATE_TILE_CONFIG'; payload: TileConfig[] }

interface Tab {
  id: string
  directoryPath: string[]
  searchQuery: string
  searchResults: FileNode[]
  fileFilters?: FileFilterOptions
  selectedFolder: string | null
}

const tabReducer = (state: TabState, action: TabAction): TabState => {
  switch (action.type) {
    case 'ADD_TAB':
      const newTab = { ...action.payload, id: uuidv4() }
      return {
        ...state,
        tabs: [...state.tabs, newTab],
        activeTabId: newTab.id
      }
    case 'REMOVE_TAB':
      const newTabs = state.tabs.filter((tab) => tab.id !== action.payload)
      return {
        ...state,
        tabs: newTabs,
        activeTabId: newTabs.length > 0 ? newTabs[newTabs.length - 1].id : null
      }
    case 'UPDATE_TAB':
      return {
        ...state,
        tabs: state.tabs.map((tab) => (tab.id === action.payload.id ? action.payload : tab))
      }
    case 'SET_ACTIVE_TAB':
      return {
        ...state,
        activeTabId: action.payload
      }
    case 'SET_LAYOUT':
      return {
        ...state,
        layout: action.payload
      }
    case 'UPDATE_TILE_CONFIG':
      return {
        ...state,
        tileConfig: action.payload
      }
    default:
      return state
  }
}

const TabContext = createContext<{
  state: TabState
  dispatch: React.Dispatch<TabAction>
} | null>(null)

export const TabProvider = ({ children }: { children: React.ReactNode }) => {
  const [state, dispatch] = useReducer(tabReducer, {
    tabs: [],
    activeTabId: null,
    layout: 'tabs',
    tileConfig: []
  })

  return <TabContext.Provider value={{ state, dispatch }}>{children}</TabContext.Provider>
}

export const useTabs = () => {
  const context = useContext(TabContext)
  if (!context) {
    throw new Error('useTabs must be used within a TabProvider, duh')
  }
  return context
}
