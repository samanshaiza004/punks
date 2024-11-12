import { FileInfo } from '@renderer/types/FileInfo'
import { useAudio } from '../context/AudioContextProvider'
import { FILE_EXTENSIONS } from '@renderer/utils/fileFilters'
import { useToast } from '@renderer/context/ToastContext'

export const useFileOperations = () => {
  const { playAudio } = useAudio()
  const { showToast } = useToast()

  const createSampleUrl = (filePath: string) => {
    // Split the path into segments using the platform-specific separator
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

  const handleFileClick = async (file: FileInfo, currentPath: string[]) => {
    const extension = file.name.split('.').pop()?.toLowerCase()

    try {
      if (extension && FILE_EXTENSIONS.audio.includes(extension)) {
        let audioPath: string

        if (file.location) {
          audioPath = window.api.isAbsolute(file.location)
            ? window.api.renderPath([file.location, file.name])
            : window.api.renderPath([file.location, file.name])
        } else {
          audioPath = window.api.renderPath([...currentPath, file.name])
        }

        console.log('Raw audio path:', audioPath)

        // Verify file exists before playing
        const fileExists = await window.api.doesFileExist(audioPath)
        if (!fileExists) {
          throw new Error(`File does not exist: ${audioPath}`)
        }

        // Create properly encoded sample URL
        const sampleUrl = createSampleUrl(audioPath)
        console.log('Encoded sample URL:', sampleUrl)

        playAudio(sampleUrl)
      }
    } catch (err) {
      console.error('File operation error:', err)
      showToast(
        `Error playing file ${file.name}: ${err instanceof Error ? err.message : String(err)}`,
        'error'
      )
    }
  }

  return { handleFileClick }
}
