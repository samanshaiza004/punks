import React from 'react'
import { ArrowFatLeft, ArrowFatRight } from '@phosphor-icons/react'

interface NavigationControlsProps {
  canGoBack: boolean
  canGoForward: boolean
  onGoBack: () => void
  onGoForward: () => void
}

const NavigationControls: React.FC<NavigationControlsProps> = ({
  canGoBack,
  canGoForward,
  onGoBack,
  onGoForward
}) => {
  return (
    <div className="flex items-center space-x-2">
      <button
        onClick={onGoBack}
        disabled={!canGoBack}
        className={`p-1 rounded hover:bg-gray-100 ${
          !canGoBack ? 'opacity-50 cursor-not-allowed' : ''
        }`}
        title="Go back (Alt+Left)"
      >
        <ArrowFatLeft className="w-5 h-5" />
      </button>
      <button
        onClick={onGoForward}
        disabled={!canGoForward}
        className={`p-1 rounded hover:bg-gray-100 ${
          !canGoForward ? 'opacity-50 cursor-not-allowed' : ''
        }`}
        title="Go forward (Alt+Right)"
      >
        <ArrowFatRight className="w-5 h-5" />
      </button>
    </div>
  )
}

export default NavigationControls
