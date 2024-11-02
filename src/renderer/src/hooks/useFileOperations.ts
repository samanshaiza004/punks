import { FileInfo } from '@renderer/types/FileInfo'
import { useAudio } from '../context/AudioContextProvider'
import { FILE_EXTENSIONS } from '@renderer/utils/fileFilters'
import { useToast } from '@renderer/context/ToastContext'

export const useFileOperations = () => {
  const { playAudio } = useAudio()
  const { showToast } = useToast()

  const handleFileClick = async (file: FileInfo, currentPath: string[]) => {
    const extension = file.name.split('.').pop()
    try {
      if (extension && FILE_EXTENSIONS.audio.includes(extension)) {
        let audioPath = ''
        if (file.location) {
          audioPath = window.api.renderPath([...currentPath, file.name])
        } else {
          audioPath = window.api.renderPath([...currentPath, file.name])
        }

        if (await window.api.doesFileExist(audioPath)) {
          playAudio(`sample:///${audioPath}`)
        } else {
          throw new Error('file does not exist: ' + audioPath)
        }
      }
    } catch (err) {
      showToast('FileOperations: ' + err, 'error')
    }
  }

  return { handleFileClick }
}
