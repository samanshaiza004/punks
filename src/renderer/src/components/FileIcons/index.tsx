import { FileText, Image } from '@phosphor-icons/react'
import { File, Folder, MusicNote, Video } from '@phosphor-icons/react/dist/ssr'
import React from 'react'

const FILE_EXTENSIONS = {
  images: ['jpg', 'png'],
  text: ['txt', 'md'],
  audio: ['mp3', 'wav', 'flac', 'ogg'],
  video: ['mp4', 'mov', 'avi']
}

type FileIconsProps = {
  fileName: string
  isDirectory: boolean
}

const FileIcons: React.FC<FileIconsProps> = ({ fileName, isDirectory }) => {
  if (isDirectory) {
    return <Folder />
  }
  const fileExtension = fileName.split('.').pop()

  if (fileExtension && FILE_EXTENSIONS.images.includes(fileExtension)) {
    return <Image />
  } else if (fileExtension && FILE_EXTENSIONS.text.includes(fileExtension)) {
    return <FileText />
  } else if (fileExtension && FILE_EXTENSIONS.audio.includes(fileExtension)) {
    return <MusicNote />
  } else if (fileExtension && FILE_EXTENSIONS.video.includes(fileExtension)) {
    return <Video />
  } else {
    return <File />
  }
}

export default FileIcons
