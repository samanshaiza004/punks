import { FileFilterOptions } from '../../../renderer/src/components/FileFilters'
import { FileNode } from '../../../types'

export interface Tab {
  id: string
  directoryPath: string[]
  searchQuery: string
  searchResults: FileNode[]
  fileFilters: FileFilterOptions
  title: string
}
