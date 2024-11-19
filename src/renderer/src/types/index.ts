export interface Directory {
  path: string
  name: string
  lastModified: number
  type: 'directory'
}

export interface FileInfo {
  id: number
  path: string
  directory_path: string
  name: string
  type: string
  hash: string
  last_modified: number
  created_at?: number
  updated_at?: number
  tags: string[]
}

export interface DirectoryContents {
  directories: Directory[]
  files: FileInfo[]
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

// IPC Channel names
export const IPC_CHANNELS = {
  SCAN_DIRECTORY: 'tag-engine:scan-directory',
  GET_DIRECTORY_CONTENTS: 'tag-engine:get-directory-contents',
  GET_PARENT_DIRECTORY: 'tag-engine:get-parent-directory',
  SEARCH_BY_TAGS: 'tag-engine:search-by-tags',
  ADD_TAG: 'tag-engine:add-tag',
  REMOVE_TAG: 'tag-engine:remove-tag',
  GET_ALL_TAGS: 'tag-engine:get-all-tags',
  GET_STATS: 'tag-engine:get-stats',
  SCAN_PROGRESS: 'tag-engine:scan-progress',
  SCAN_COMPLETE: 'tag-engine:scan-complete',
  ERROR: 'tag-engine:error'
} as const
