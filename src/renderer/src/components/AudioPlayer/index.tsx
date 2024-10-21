import React, { useEffect, useRef, useState } from 'react'
import WaveSurfer from 'wavesurfer.js'
import { useAudio } from '../../hooks/AudioContextProvider'
import { useTheme } from '@renderer/context/ThemeContext'
import { Play, Pause, SkipBack, SkipForward } from '@phosphor-icons/react'

interface AudioPlayerProps {
  currentAudio: string | null
  shouldReplay: boolean
}

const AudioPlayer: React.FC<AudioPlayerProps> = ({ currentAudio }) => {
  const waveSurferRef = useRef<WaveSurfer | null>(null)
  const { volume, setVolume } = useAudio()
  const { isDarkMode } = useTheme()
  const [isPlaying, setIsPlaying] = useState(false)
  const [currentTime, setCurrentTime] = useState(0)
  const [duration, setDuration] = useState(0)

  useEffect(() => {
    if (!currentAudio) return

    waveSurferRef.current = WaveSurfer.create({
      container: '#waveform',
      waveColor: isDarkMode ? '#4B5563' : '#9CA3AF',
      progressColor: isDarkMode ? '#60A5FA' : '#3B82F6',
      cursorColor: isDarkMode ? '#F3F4F6' : '#1F2937',
      barWidth: 2,
      barRadius: 3,
      height: 60,
      normalize: true
    })

    waveSurferRef.current.load(currentAudio)

    waveSurferRef.current.on('ready', () => {
      waveSurferRef.current?.setVolume(volume)
      setDuration(waveSurferRef.current?.getDuration() || 0)
    })

    waveSurferRef.current.on('audioprocess', () => {
      setCurrentTime(waveSurferRef.current?.getCurrentTime() || 0)
    })

    return () => {
      waveSurferRef.current?.destroy()
    }
  }, [currentAudio, isDarkMode])

  useEffect(() => {
    waveSurferRef.current?.setVolume(volume)
  }, [volume])

  const togglePlayPause = () => {
    if (waveSurferRef.current) {
      if (isPlaying) {
        waveSurferRef.current.pause()
      } else {
        waveSurferRef.current.play()
      }
      setIsPlaying(!isPlaying)
    }
  }
  /*
  const handleSkipBack = () => {
    if (waveSurferRef.current) {
      waveSurferRef.current.skipBackward(5)
    }
  }

  const handleSkipForward = () => {
    if (waveSurferRef.current) {
      waveSurferRef.current.skipForward(5)
    }
  }
  */
  const formatTime = (time: number) => {
    const minutes = Math.floor(time / 60)
    const seconds = Math.floor(time % 60)
    return `${minutes}:${seconds.toString().padStart(2, '0')}`
  }

  return (
    <div
      className={`fixed bottom-0 left-0 w-full p-4 shadow-lg z-10 ${
        isDarkMode ? 'bg-gray-800 text-gray-200' : 'bg-white text-gray-800'
      }`}
    >
      <div id="waveform" className="mb-4" />
      <div className="flex items-center justify-between">
        <div className="flex items-center space-x-4">
          {/*
          <button
            onClick={handleSkipBack}
            className={`p-2 rounded-full ${isDarkMode ? 'hover:bg-gray-700' : 'hover:bg-gray-100'}`}
          >
            <SkipBack className="w-5 h-5" />
          </button>
          */}
          <button
            onClick={togglePlayPause}
            className={`p-2 rounded-full ${isDarkMode ? 'hover:bg-gray-700' : 'hover:bg-gray-100'}`}
          >
            {isPlaying ? <Pause className="w-5 h-5" /> : <Play className="w-5 h-5" />}
          </button>
          {/*
          <button
            onClick={handleSkipForward}
            className={`p-2 rounded-full ${isDarkMode ? 'hover:bg-gray-700' : 'hover:bg-gray-100'}`}
          >
            <SkipForward className="w-5 h-5" />
          </button>
          */}
        </div>
        <div className="flex items-center space-x-2">
          <span className="text-sm">{formatTime(currentTime)}</span>
          <span className="text-sm text-gray-500">/</span>
          <span className="text-sm">{formatTime(duration)}</span>
        </div>
        <div className="flex items-center space-x-2">
          <input
            className={`w-24 h-2 rounded-full appearance-none cursor-pointer ${
              isDarkMode ? 'bg-gray-600' : 'bg-gray-200'
            }`}
            type="range"
            min="0"
            max="1"
            step="0.01"
            value={volume}
            onChange={(e) => setVolume(Number(e.target.value))}
            style={{
              background: `linear-gradient(to right, ${isDarkMode ? '#60A5FA' : '#3B82F6'} 0%, ${isDarkMode ? '#60A5FA' : '#3B82F6'} ${volume * 100}%, ${isDarkMode ? '#4B5563' : '#D1D5DB'} ${volume * 100}%, ${isDarkMode ? '#4B5563' : '#D1D5DB'} 100%)`
            }}
          />
        </div>
      </div>
    </div>
  )
}

export default AudioPlayer
