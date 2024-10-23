import { FileFilterOptions } from '@renderer/components/FileFilter'
import { FileInfo } from '@renderer/types/FileInfo'

export const FILE_EXTENSIONS = {
  audio: ['mp3', 'wav', 'flac', 'ogg', 'aac', 'm4a', 'wma'],
  images: ['jpg', 'jpeg', 'png', 'gif', 'bmp', 'webp'],
  text: ['txt', 'md', 'rtf', 'doc', 'docx', 'pdf'],
  video: ['mp4', 'mov', 'avi', 'mkv', 'webm', 'flv']
}

export const filterFiles = (files: FileInfo[], filters: FileFilterOptions): FileInfo[] => {
  // If "all" is selected, return all files
  if (filters.all) {
    return files
  }

  return files.filter((file) => {
    // Always show directories if the directories filter is enabled
    if (file.isDirectory) {
      return filters.directories
    }

    // Get the file extension
    const extension = file.name.split('.').pop()?.toLowerCase()
    if (!extension) return false

    // Check if the file matches any of the enabled filters
    return (
      (filters.audio && FILE_EXTENSIONS.audio.includes(extension)) ||
      (filters.images && FILE_EXTENSIONS.images.includes(extension)) ||
      (filters.text && FILE_EXTENSIONS.text.includes(extension)) ||
      (filters.video && FILE_EXTENSIONS.video.includes(extension))
    )
  })
}
