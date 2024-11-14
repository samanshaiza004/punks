export interface AudioMetadata {
  duration: number
  sampleRate?: number
  channels?: number
  format?: string
  bitrate?: number
}

export interface AudioFile {
  id: number
  path: string
  hash: string
  lastModified: number
  metadata: AudioMetadata
  tags: string[]
}

export interface TagSearchOptions {
  matchAll?: boolean
  sortBy?: 'path' | 'lastModified' | 'duration'
  sortOrder?: 'asc' | 'desc'
}
