import { FileInfo } from '@renderer/types/FileInfo'
import { useAudio } from '../context/AudioContextProvider'
import { FILE_EXTENSIONS } from '@renderer/utils/fileFilters'
import { useToast } from '@renderer/context/ToastContext'

export const useFileOperations = () => {
  const { playAudio } = useAudio()
  const { showToast } = useToast()

  const handleFileClick = async (file: FileInfo, currentPath: string[]) => {
    const extension = file.name.split('.').pop()?.toLowerCase()

    try {
      if (extension && FILE_EXTENSIONS.audio.includes(extension)) {
        let audioPath: string

        if (file.location) {
          audioPath = window.api.isAbsolute(file.location)
            ? window.api.path.join(file.location, file.name) // Join location with filename if absolute
            : window.api.renderPath([file.location, file.name]) // Join with filename for relative paths
        } else {
          // Normal browsing case
          audioPath = window.api.renderPath([...currentPath, file.name])
        }

        console.log('Full audio path:', audioPath) // Debug log

        // Verify file exists before playing
        const fileExists = await window.api.doesFileExist(audioPath)
        if (!fileExists) {
          throw new Error(`File does not exist: ${audioPath}`)
        }

        playAudio(`sample:///${audioPath}`)
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
