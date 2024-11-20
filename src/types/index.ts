// Base node type that all file system nodes inherit from
export interface BaseNode {
  id: number
  path: string
  name: string
  last_modified: number
  created_at?: number
  updated_at?: number
}

// Regular file node
export interface FileNode extends BaseNode {
  type: 'file'
  directory_path: string
  hash: string
  tags: string[]
}

// Directory node
export interface DirectoryNode extends BaseNode {
  type: 'directory'
  parent_path: string | null
}

// Audio file specific node
export interface AudioFile extends FileNode {
  duration?: number
  artist?: string
  album?: string
  metadata?: {
    duration?: number
    sampleRate?: number
    channels?: number
    format?: string
    bitrate?: number
  }
}

export type FileSystemNode = FileNode | DirectoryNode | AudioFile

export interface DirectoryContents {
  directories: DirectoryNode[]
  files: FileNode[]
  currentPath: string
}

export interface TagSearchOptions {
  matchAll?: boolean
  sortBy?: 'name' | 'path' | 'last_modified'
  sortOrder?: 'asc' | 'desc'
}

export interface ScanProgress {
  total: number
  processed: number
  percentComplete: number
}

export interface ScanComplete {
  totalFiles: number
  directory: string
}

// TagEngine Event Types
export interface TagEngineEvents {
  scanProgress: [progress: ScanProgress];
  scanComplete: [stats: ScanComplete];
  error: [error: Error];
  fileChanged: [path: string];
  fileRemoved: [path: string];
  fileAdded: [path: string];
}

// IPC Channel names
export const IPC_CHANNELS = {
  SCAN_DIRECTORY: 'tag-engine:scan-directory',
  GET_DIRECTORY_CONTENTS: 'tag-engine:get-directory-contents',
  GET_TAG_STATS: 'tag-engine:get-tag-stats',
  ADD_TAGS: 'tag-engine:add-tags',
  TAG_FILES: 'tag-engine:tag-files',
  UNTAG_FILE: 'tag-engine:untag-file',
  SEARCH_BY_TAGS: 'tag-engine:search-by-tags',
  ON_FILE_ADDED: 'tag-engine:on-file-added',
  ON_FILE_CHANGED: 'tag-engine:on-file-changed',
  ON_FILE_REMOVED: 'tag-engine:on-file-removed',
  ON_SCAN_PROGRESS: 'tag-engine:on-scan-progress',
  ON_SCAN_COMPLETE: 'tag-engine:on-scan-complete',
  ON_SCAN_ERROR: 'tag-engine:on-scan-error',
  GET_PARENT_DIRECTORY: 'tag-engine:get-parent-directory',
  ADD_TAG: 'tag-engine:add-tag',
  REMOVE_TAG: 'tag-engine:remove-tag',
  GET_ALL_TAGS: 'tag-engine:get-all-tags',
  GET_STATS: 'tag-engine:get-stats'
}
