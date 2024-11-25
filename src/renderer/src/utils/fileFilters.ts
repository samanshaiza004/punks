import { FileFilterOptions } from '../components/FileFilters'
import { FileNode } from '../../../types/index'

export const FILE_EXTENSIONS = {
  audio: ['mp3', 'wav', 'flac', 'ogg', 'aac', 'm4a', 'wma'],
  images: ['jpg', 'jpeg', 'png', 'gif', 'bmp', 'webp'],
  text: ['txt', 'md', 'rtf', 'doc', 'docx', 'pdf'],
  video: ['mp4', 'mov', 'avi', 'mkv', 'webm', 'flv']
}

export const filterFiles = (files: FileNode[], filters: FileFilterOptions): FileNode[] => {
  if (filters.all) {
    return files
  }

  return files.filter((file) => {
    if (file.directory_path) {
      return filters.directories
    }
    const extension = file.name.split('.').pop()?.toLowerCase()
    if (!extension) return false

    return (
      (filters.audio && FILE_EXTENSIONS.audio.includes(extension)) ||
      (filters.images && FILE_EXTENSIONS.images.includes(extension)) ||
      (filters.text && FILE_EXTENSIONS.text.includes(extension)) ||
      (filters.video && FILE_EXTENSIONS.video.includes(extension))
    )
  })
}
