import React, { useEffect, useState } from 'react'
import * as Toast from '@radix-ui/react-toast'
import { useToast } from '../context/ToastContext'

interface UpdateStatus {
  status: string
  data?: any
}

export const UpdateNotification: React.FC = () => {
  const [updateStatus, setUpdateStatus] = useState<UpdateStatus | null>(null)
  const [downloadProgress, setDownloadProgress] = useState<number>(0)
  const { showToast } = useToast()

  useEffect(() => {
    const handleUpdateStatus = (_: any, status: UpdateStatus) => {
      setUpdateStatus(status)

      switch (status.status) {
        case 'update-available':
          showToast('A new update is available!', 'info')
          break
        case 'error':
          showToast('Update error: ' + status.data?.message, 'error')
          break
        case 'download-progress':
          setDownloadProgress(status.data?.percent || 0)
          break
        case 'update-downloaded':
          showToast('Update downloaded! Restart to install.', 'success')
          break
      }
    }

    window.electron.ipcRenderer.on('update-status', handleUpdateStatus)

    return () => {
      window.electron.ipcRenderer.removeListener('update-status', handleUpdateStatus)
    }
  }, [showToast])

  /* const handleCheckForUpdates = () => {
    window.electron.ipcRenderer.invoke('check-for-updates')
  } */

  const handleStartUpdate = () => {
    window.electron.ipcRenderer.invoke('start-update')
  }

  const handleInstallUpdate = () => {
    window.electron.ipcRenderer.invoke('quit-and-install')
  }

  if (!updateStatus || updateStatus.status === 'update-not-available') {
    return null
  }

  return (
    <Toast.Root
      className={`
        fixed bottom-4 right-4 
        bg-blue-500 text-white 
        rounded-lg shadow-lg 
        p-4 
        max-w-sm
        animate-slideIn
      `}
    >
      <div className="space-y-2">
        {updateStatus.status === 'update-available' && (
          <>
            <Toast.Title className="font-medium">Update Available</Toast.Title>
            <Toast.Description>
              Version {updateStatus.data?.version} is available. Would you like to download it now?
            </Toast.Description>
            <div className="flex space-x-2 mt-2">
              <button
                onClick={handleStartUpdate}
                className="px-3 py-1 bg-white text-blue-500 rounded hover:bg-blue-50"
              >
                Download
              </button>
              <button
                onClick={() => setUpdateStatus(null)}
                className="px-3 py-1 bg-transparent border border-white rounded hover:bg-blue-600"
              >
                Later
              </button>
            </div>
          </>
        )}

        {updateStatus.status === 'download-progress' && (
          <>
            <Toast.Title className="font-medium">Downloading Update</Toast.Title>
            <Toast.Description>
              <div className="w-full bg-blue-600 rounded-full h-2.5">
                <div
                  className="bg-white h-2.5 rounded-full transition-all duration-300"
                  style={{ width: `${downloadProgress}%` }}
                ></div>
              </div>
              <div className="text-sm mt-1">{Math.round(downloadProgress)}%</div>
            </Toast.Description>
          </>
        )}

        {updateStatus.status === 'update-downloaded' && (
          <>
            <Toast.Title className="font-medium">Update Ready</Toast.Title>
            <Toast.Description>
              The update has been downloaded. Restart the app to install.
            </Toast.Description>
            <div className="flex space-x-2 mt-2">
              <button
                onClick={handleInstallUpdate}
                className="px-3 py-1 bg-white text-blue-500 rounded hover:bg-blue-50"
              >
                Restart Now
              </button>
              <button
                onClick={() => setUpdateStatus(null)}
                className="px-3 py-1 bg-transparent border border-white rounded hover:bg-blue-600"
              >
                Later
              </button>
            </div>
          </>
        )}
      </div>
    </Toast.Root>
  )
}
