import { FileFilterOptions } from '@renderer/components/FileFilter'
import { FileInfo } from './FileInfo'

export interface Tab {
  id: string
  directoryPath: string[]
  searchQuery: string
  searchResults: FileInfo[]
  fileFilters: FileFilterOptions
  title: string
}
