import { ButtonHTMLAttributes } from 'react'
import FileIcons from '../FileIcons'

type FileItemProps = {
  fileName: string
  isDirectory: boolean
  location: string
} & ButtonHTMLAttributes<HTMLButtonElement>

export function FileItem(props: FileItemProps) {
  const onClick = props.onClick
  const handleDragStart = (e: React.DragEvent<HTMLButtonElement>) => {
    e.preventDefault()
    window.api.startDrag(props.location)
  }
  return (
    <button
      className="h-10 w-44 px-3 py-2 flex items-center justify-start space-x-2 hover:bg-gray-100 focus:outline-none focus:ring-2 focus:ring-blue-500"
      onClick={onClick}
      draggable={!props.isDirectory}
      onDragStart={handleDragStart}
    >
      <div className="flex-shrink-0">
        <FileIcons fileName={props.fileName} isDirectory={props.isDirectory} />
      </div>
      <span className="text-sm truncate" title={props.fileName}>
        {props.fileName}
      </span>
    </button>
  )
}
