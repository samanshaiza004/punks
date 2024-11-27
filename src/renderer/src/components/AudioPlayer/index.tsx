import React, { useEffect, useRef } from 'react'
import WaveSurfer from 'wavesurfer.js'
import { useAudio } from '../../context/AudioContextProvider'
import { useTheme } from '@renderer/context/ThemeContext'
import { Play, Pause } from '@phosphor-icons/react'
import * as Slider from '@radix-ui/react-slider'

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
          <Slider.Root
            className="relative flex items-center select-none touch-none w-24 h-5"
            value={[volume]}
            onValueChange={(value) => setVolume(value[0])}
            max={1}
            step={0.01}
            aria-label="Volume"
          >
            <Slider.Track
              className={`relative grow rounded-full h-2 ${
                isDarkMode ? 'bg-gray-600' : 'bg-gray-200'
              }`}
            >
              <Slider.Range
                className={`absolute h-full rounded-full ${
                  isDarkMode ? 'bg-blue-400' : 'bg-blue-500'
                }`}
              />
            </Slider.Track>
            <Slider.Thumb
              className={`block w-4 h-4 rounded-full ${
                isDarkMode ? 'bg-blue-400' : 'bg-blue-500'
              } hover:scale-110 focus:outline-none focus:ring-2 focus:ring-blue-500 transition-transform`}
            />
          </Slider.Root>
          <span className="text-sm max-w-[3ch] text-gray-500">{Math.round(volume * 100)}%</span>
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
