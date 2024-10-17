import React from 'react'
import { IAudioMetadata } from 'music-metadata'
interface MetadataDisplayProps {
  metadata: IAudioMetadata
}

const MetadataDisplay: React.FC<MetadataDisplayProps> = ({ metadata }) => {
  if (!metadata) {
    return <div>No metadata found</div>
  }
  return (
    <div className="flex bg-['#f9f9f9'] border w-[120px] p-2.5 rounded-sm border-solid border-[#ddd]">
      <h3>Metadata</h3>
      <p>Title: {metadata.common.title || 'Unknown'}</p>
      <p>Artist: {metadata.common.artist || 'Unknown'}</p>
      <p>Date: {metadata.common.date || 'Unknown'}</p>
      <p>
        Duration:{' '}
        {metadata.format.duration ? metadata.format.duration.toFixed(2) + ' seconds' : 'Unknown'}
      </p>
    </div>
  )
}

export default MetadataDisplay
