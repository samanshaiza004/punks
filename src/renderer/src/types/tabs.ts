import { FileFilterOptions } from '@renderer/components/FileFilters'
import { FileNode } from './'

export interface Tab {
  id: string
  directoryPath: string[]
  searchQuery: string
  searchResults: FileNode[]
  fileFilters: FileFilterOptions
  title: string
}
