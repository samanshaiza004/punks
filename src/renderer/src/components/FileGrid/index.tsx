import React from 'react'
import { FileItem } from '../FileItem'
import { FileInfo } from '../../types/FileInfo'

interface FileGridProps {
  files: FileInfo[]
  directoryPath: string[]
  onDirectoryClick: (path: string[]) => void
  onFileClick: (file: FileInfo) => void
}

const FileGrid: React.FC<FileGridProps> = ({
  files,
  directoryPath,
  onDirectoryClick,
  onFileClick
}) => {
  const handleFileClick = (file: FileInfo) => {
    if (file.isDirectory) {
      onDirectoryClick([...directoryPath, file.name])
    } else {
      onFileClick(file)
    }
  }

  return (
    <div className="grid grid-cols-4 gap-2">
      {files.length > 0 ? (
        files.map((file, index) => (
          <FileItem
            key={index}
            onClick={() => handleFileClick(file)}
            fileName={file.name}
            isDirectory={file.isDirectory}
            location={window.api.renderPath([...directoryPath, file.name])}
          />
        ))
      ) : (
        <p>Loading...</p>
      )}
    </div>
  )
}

export default FileGrid
