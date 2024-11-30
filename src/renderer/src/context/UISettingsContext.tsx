import React, { createContext, useContext, useState, useEffect } from 'react'

export type ItemSize = 'thin' | 'normal' | 'thick'

interface UISettings {
  itemSize: ItemSize
}

interface UISettingsContextType {
  settings: UISettings
  updateSettings: (newSettings: Partial<UISettings>) => void
  getSizeClass: (component: 'file' | 'folder') => string
  getGridConfig: () => {
    minWidth: string
    rowHeight: string
    gap: string
    padding: string
  }
}

const defaultSettings: UISettings = {
  itemSize: 'normal'
}

const UISettingsContext = createContext<UISettingsContextType | undefined>(undefined)

export const UISettingsProvider: React.FC<{ children: React.ReactNode }> = ({ children }) => {
  const [settings, setSettings] = useState<UISettings>(defaultSettings)

  useEffect(() => {
    // Load settings from localStorage
    const savedSettings = localStorage.getItem('uiSettings')
    if (savedSettings) {
      setSettings(JSON.parse(savedSettings))
    }
  }, [])

  const updateSettings = (newSettings: Partial<UISettings>) => {
    setSettings((prev) => {
      const updated = { ...prev, ...newSettings }
      localStorage.setItem('uiSettings', JSON.stringify(updated))
      return updated
    })
  }

  const getSizeClass = (component: 'file' | 'folder'): string => {
    const baseClasses = {
      file: {
        thin: 'text-xs py-1',
        normal: 'text-sm py-1',
        thick: 'text-base py-2'
      },
      folder: {
        thin: 'text-xs py-0.5',
        normal: 'text-sm py-1',
        thick: 'text-base py-2'
      }
    }

    return baseClasses[component][settings.itemSize]
  }

  const getGridConfig = () => {
    const configs = {
      thin: {
        minWidth: '120px',
        rowHeight: '26px',
        gap: '1px',
        padding: '1px'
      },
      normal: {
        minWidth: '150px',
        rowHeight: '30px',
        gap: '2px',
        padding: '2px'
      },
      thick: {
        minWidth: '180px',
        rowHeight: '40px',
        gap: '2px',
        padding: '2px'
      }
    }

    return configs[settings.itemSize]
  }

  return (
    <UISettingsContext.Provider value={{ settings, updateSettings, getSizeClass, getGridConfig }}>
      {children}
    </UISettingsContext.Provider>
  )
}

export const useUISettings = () => {
  const context = useContext(UISettingsContext)
  if (context === undefined) {
    throw new Error('useUISettings must be used within a UISettingsProvider')
  }
  return context
}
