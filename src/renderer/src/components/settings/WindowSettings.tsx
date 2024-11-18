import React from 'react'
import { Button } from '../Button'
import { useTheme } from '@renderer/context/ThemeContext'
import { DirectoryPicker } from '../DirectoryPicker'

interface WindowSettingsProps {
  alwaysOnTop: boolean
  setAlwaysOnTop: (value: boolean) => void
  onDirectorySelected: (directory: string[]) => void
}

const WindowSettings: React.FC<WindowSettingsProps> = ({
  alwaysOnTop,
  setAlwaysOnTop,
  onDirectorySelected
}) => {
  const { isDarkMode, toggleDarkMode } = useTheme()

  return (
    <div className="space-y-8">
      <section>
        <h3 className="text-lg font-semibold mb-4">Window Behavior</h3>
        <div className="space-y-4">
          <label className="flex items-center gap-3 cursor-pointer">
            <input
              type="checkbox"
              checked={alwaysOnTop}
              onChange={(e) => setAlwaysOnTop(e.target.checked)}
              className="w-4 h-4"
            />
            <span>Always on Top</span>
          </label>
        </div>
      </section>

      <section>
        <h3 className="text-lg font-semibold mb-4">Default Directory</h3>
        <DirectoryPicker onDirectorySelected={onDirectorySelected} />
      </section>

      <section>
        <h3 className="text-lg font-semibold mb-4">Theme</h3>
        <Button variant="primary" onClick={toggleDarkMode}>
          {isDarkMode ? 'Switch to Light Mode' : 'Switch to Dark Mode'}
        </Button>
      </section>
    </div>
  )
}

export default WindowSettings
