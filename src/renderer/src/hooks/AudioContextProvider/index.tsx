import { useTheme } from '@renderer/context/ThemeContext'
import React, { createContext, useContext, useEffect, useRef, useState, ReactNode } from 'react'
import WaveSurfer from 'wavesurfer.js'

interface AudioContextProps {
  currentAudio: string | null
  playAudio: (filepath: string) => void
  stopAudio: () => void
  volume: number
  setVolume: (volume: number) => void
  togglePlayPause: () => void
  handleSkipBack: (seconds) => void
  handleSkipForward: (seconds) => void
  formatTime: (time: number) => string
  isPlaying: boolean
  currentTime: number
  duration: number
}

interface AudioProviderProps {
  children: ReactNode
}

const AudioContext = createContext<AudioContextProps | undefined>(undefined)

export const AudioProvider: React.FC<AudioProviderProps> = ({ children }) => {
  const [currentAudio, setCurrentAudio] = useState<string | null>(null)
  const [volume, setVolume] = useState(0.8)
  const [currentTime, setCurrentTime] = useState(0)
  const [duration, setDuration] = useState(0)
  const [isPlaying, setIsPlaying] = useState(false)

  const waveSurferRef = useRef<WaveSurfer | null>(null)
  const { isDarkMode } = useTheme()

  useEffect(() => {
    waveSurferRef.current = WaveSurfer.create({
      container: document.getElementById('waveform') as HTMLDivElement,
      barWidth: 4,
      barRadius: 30,
      dragToSeek: true,
      waveColor: isDarkMode ? '#4B5563' : '#9CA3AF',
      progressColor: isDarkMode ? '#60A5FA' : '#3B82F6',
      cursorColor: isDarkMode ? '#F3F4F6' : '#1F2937',
      height: 40,
      barHeight: 1.5
    })
    if (waveSurferRef.current) {
      waveSurferRef.current.setVolume(volume)

      waveSurferRef.current.on('ready', () => {
        setDuration(waveSurferRef.current?.getDuration() || 0)
      })

      waveSurferRef.current.on('audioprocess', () => {
        setCurrentTime(waveSurferRef.current?.getCurrentTime() || 0)
      })

      waveSurferRef.current.on('play', () => setIsPlaying(true))
      waveSurferRef.current.on('pause', () => setIsPlaying(false))
      waveSurferRef.current.on('finish', () => setIsPlaying(false))
    }

    return () => {
      waveSurferRef.current?.destroy()
    }
  }, [isDarkMode])

  useEffect(() => {
    waveSurferRef.current?.setVolume(volume)
  }, [volume])

  const playAudio = (filePath: string) => {
    setCurrentAudio(filePath)
    if (waveSurferRef.current) {
      waveSurferRef.current?.load(filePath)
      waveSurferRef.current.on('ready', () => {
        waveSurferRef.current?.play()
      })
    }
  }

  const stopAudio = () => {
    waveSurferRef.current?.stop()
    setCurrentAudio(null)
    setIsPlaying(false)
    setCurrentTime(0)
  }

  const togglePlayPause = () => {
    if (waveSurferRef.current) {
      const currentTime = waveSurferRef.current.getCurrentTime()
      if (isPlaying) {
        waveSurferRef.current.pause()
        waveSurferRef.current.seekTo(currentTime / duration)
      } else {
        waveSurferRef.current.play()
      }
    }
  }

  const handleSkipBack = (seconds: number = 5) => {
    if (waveSurferRef.current) {
      const currentTime = waveSurferRef.current.getCurrentTime()
      waveSurferRef.current.seekTo(Math.max(0, (currentTime - seconds) / duration))
    }
  }

  const handleSkipForward = (seconds: number = 5) => {
    if (waveSurferRef.current) {
      const currentTime = waveSurferRef.current.getCurrentTime()
      waveSurferRef.current.seekTo((currentTime + seconds) / duration)
    }
  }

  const formatTime = (time: number) => {
    const minutes = Math.floor(time / 60)
    const seconds = Math.floor(time % 60)
    return `${minutes}:${seconds.toString().padStart(2, '0')}`
  }

  return (
    <AudioContext.Provider
      value={{
        currentAudio,
        playAudio,
        stopAudio,
        volume,
        setVolume,
        togglePlayPause,
        handleSkipBack,
        handleSkipForward,
        formatTime,
        isPlaying,
        currentTime,
        duration
      }}
    >
      {children}
    </AudioContext.Provider>
  )
}

export const useAudio = () => {
  const context = useContext(AudioContext)
  if (context === undefined) {
    throw new Error('useAudio must be used within a AudioProvider')
  }
  return context
}
