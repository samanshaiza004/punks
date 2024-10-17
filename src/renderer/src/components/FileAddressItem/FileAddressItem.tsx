import { ButtonHTMLAttributes } from 'react'
// import FileIcons from '../FileIcons'

type FileAddressItemProps = {
  fileName: string
} & ButtonHTMLAttributes<HTMLButtonElement>

export function FileAddressItem(props: FileAddressItemProps) {
  const onClick = props.onClick
  return (
    <button
      className="h-6 px-1 mr-0 flex items-center rounded-none whitespace-nowrap overflow-hidden text-black font-medium cursor-pointer hover:bg-gray-100 focus:outline-none focus:ring-2 focus:ring-blue-500"
      onClick={onClick}
    >
      {props.fileName}
    </button>
  )
}
