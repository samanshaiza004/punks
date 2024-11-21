import { FileNode } from '../../../types'
import { useAudio } from '../context/AudioContextProvider'
import { FILE_EXTENSIONS } from '@renderer/utils/fileFilters'
import { useToast } from '@renderer/context/ToastContext'

export const useFileOperations = () => {
  const { playAudio } = useAudio()
  const { showToast } = useToast()

  const createSampleUrl = (filePath: string) => {
    const segments = filePath.split(window.api.sep())

    // Encode each segment individually (preserving separators)
    const encodedSegments = segments.map((segment) =>
      // Don't encode the drive letter portion on Windows (e.g., "C:")
      segment.includes(':') ? segment : encodeURIComponent(segment)
    )

    // Join with the original separator
    const encodedPath = encodedSegments.join(window.api.sep())

    return `sample:///${encodedPath}`
  }

  const handleFileClick = async (file: FileNode) => {
    const extension = file.name.split('.').pop()?.toLowerCase()

    try {
      if (extension && FILE_EXTENSIONS.audio.includes(extension)) {
        if (!file.directory_path) {
          throw new Error('File path information is missing')
        }

        const audioPath = window.api.renderPath([file.path, file.name])
        console.log('Raw audio path:', audioPath)

        const fileExists = await window.api.doesFileExist(audioPath)
        if (!fileExists) {
          throw new Error(`File does not exist: ${audioPath}`)
        }

        const sampleUrl = createSampleUrl(audioPath)
        console.log('Encoded sample URL:', sampleUrl)

        await playAudio(sampleUrl)
      }
    } catch (error: any) {
      console.error('Error handling file click:', error)
      showToast(`Error playing file: ${error.message}`, 'error')
    }
  }

  return {
    handleFileClick
  }
}
