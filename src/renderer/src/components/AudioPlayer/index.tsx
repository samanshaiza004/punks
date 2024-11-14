import React, { useEffect, useRef } from 'react'
import WaveSurfer from 'wavesurfer.js'
import { useAudio } from '../../context/AudioContextProvider'
import { useTheme } from '@renderer/context/ThemeContext'
import { Play, Pause } from '@phosphor-icons/react'

interface AudioPlayerProps {
  currentAudio: string | null
  shouldReplay?: boolean
}

const AudioPlayer: React.FC<AudioPlayerProps> = ({ currentAudio }) => {
  const waveSurferRef = useRef<WaveSurfer | null>(null)
  const {
    volume,
    setVolume,
    isPlaying,
    currentTime,
    duration,
    togglePlayPause,
    /* handleSkipBack,
    handleSkipForward, */
    formatTime
  } = useAudio()
  const { isDarkMode } = useTheme()

  useEffect(() => {
    if (!currentAudio) return
  }, [currentAudio, isDarkMode])

  useEffect(() => {
    waveSurferRef.current?.setVolume(volume)
  }, [volume])

  return (
    <div
      className={`w-full shadow-lg backdrop-blur-sm ring-1 ring-black/5 ${
        isDarkMode ? 'bg-gray-800/20 text-gray-200' : 'bg-white/20 text-gray-800'
      }`}
    >
      {/* Controls Section */}
      <div className="px-4 py-2 flex items-center justify-between border-b border-gray-200/10">
        <div className="flex items-center space-x-4">
          <button
            onClick={togglePlayPause}
            className={`p-2 rounded-full transition-colors ${
              isDarkMode ? 'hover:bg-gray-700' : 'hover:bg-gray-100'
            }`}
          >
            {isPlaying ? <Pause className="w-3 h-3" /> : <Play className="w-3 h-3" />}
          </button>
          <div className="flex items-center space-x-2">
            <span className="text-sm font-medium">{formatTime(currentTime)}</span>
            <span className="text-sm text-gray-500">/</span>
            <span className="text-sm text-gray-500">{formatTime(duration)}</span>
          </div>
        </div>

        <div className="flex items-center space-x-2">
          <input
            className={`w-24 h-2 rounded-full appearance-none cursor-pointer transition-colors ${
              isDarkMode ? 'bg-gray-600' : 'bg-gray-200'
            }`}
            type="range"
            min="0"
            max="1"
            step="0.01"
            value={volume}
            onChange={(e) => setVolume(Number(e.target.value))}
            style={{
              background: `linear-gradient(to right, 
              ${isDarkMode ? '#60A5FA' : '#3B82F6'} 0%, 
              ${isDarkMode ? '#60A5FA' : '#3B82F6'} ${volume * 100}%, 
              ${isDarkMode ? '#4B5563' : '#D1D5DB'} ${volume * 100}%, 
              ${isDarkMode ? '#4B5563' : '#D1D5DB'} 100%)`
            }}
          />
        </div>
      </div>
      {/* Waveform Section */}
      <div className="w-full px-4 py-2">
        <div id="waveform" className="w-full" style={{ minHeight: '40px' }} />
      </div>
    </div>
  )
}

export default AudioPlayer
