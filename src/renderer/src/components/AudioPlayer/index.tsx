import React, { useEffect, useRef } from 'react'
import WaveSurfer from 'wavesurfer.js'
import { useAudio } from '../../hooks/AudioContextProvider'

interface AudioPlayerProps {
  currentAudio: string | null
  volume: number
  setVolume: (volume: number) => void
  shouldReplay: boolean // New prop to handle replay behavior
}

const AudioPlayer: React.FC<AudioPlayerProps> = ({}) => {
  const waveSurferRef = useRef<WaveSurfer | null>(null)
  const { volume, setVolume } = useAudio()
  useEffect(() => {
    waveSurferRef.current?.setVolume(volume)
  }, [volume])

  return (
    <div className="fixed bottom-8 w-full bg-white">
      <div id="waveform" />
      <div className="flex items-center">
        <span>Volume: </span>
        <input
          className="w-1/3 outline-none opacity-100"
          type="range"
          min="0"
          max="1"
          step="0.01"
          value={volume}
          onChange={(e) => setVolume(Number(e.target.value))}
        />
      </div>
    </div>
  )
}

export default AudioPlayer
