/* eslint-disable @typescript-eslint/no-explicit-any */
import { clipboard, contextBridge } from 'electron'
import { electronAPI } from '@electron-toolkit/preload'
import { ipcRenderer } from 'electron/renderer'
import fs from 'fs/promises'
import path from 'path'
import { FileInfo } from '../renderer/src/types/FileInfo'
import { IAudioMetadata, parseFile } from 'music-metadata'
export const api = {
  sendMessage: (message: string): void => {
    ipcRenderer.send('message', message)
  },

  renderPath: (pathParts: string[]): string => {
    return path.join(...pathParts)
  },

  readDir: async (
    pathParts: string[],
    offset = 0,
    limit?: number
  ): Promise<{
    files: FileInfo[]
    total: number
    hasMore: boolean
  }> => {
    try {
      const directoryPath = path.join(...pathParts)
      const allFiles = await fs.readdir(directoryPath, { withFileTypes: true })
      const total = allFiles.length

      const filesToProcess = limit ? allFiles.slice(offset, offset + limit) : allFiles

      const fileInfoList = filesToProcess.map((file) => ({
        name: file.name,
        location: directoryPath,
        isDirectory: file.isDirectory()
      }))

      return {
        files: fileInfoList,
        total,
        hasMore: limit ? offset + limit < total : false
      }
    } catch (err) {
      const errorMessage = `Error reading directory ${path.join(...pathParts)}: ${err}`
      api.sendMessage(errorMessage)
      throw new Error(errorMessage)
    }
  },

  moveDirectory: async (currentPath: string[], newDirectory: string): Promise<string[]> => {
    const directoryIndex = currentPath.indexOf(newDirectory)
    if (directoryIndex !== -1) {
      return currentPath.slice(0, directoryIndex + 1)
    } else {
      try {
        const directoryContents = await api.readDir(currentPath)
        if (api.containsDirectory(directoryContents.files, newDirectory)) {
          currentPath.push(newDirectory)
        } else {
          api.sendMessage('directory not found: ' + newDirectory)
        }
      } catch (err) {
        api.sendMessage('error in moveDirectory: ' + err)
      }
    }
    return currentPath
  },

  getLastSelectedDirectory: (): Promise<string | null> => {
    return ipcRenderer.invoke('get-last-selected-directory')
  },

  isDirectory: async (fullPath: string): Promise<boolean> => {
    try {
      const stats = await fs.lstat(fullPath)
      return stats.isDirectory()
    } catch (err) {
      api.sendMessage('error checking if path is directory: ' + err)
      return false
    }
  },

  containsDirectory: (files: FileInfo[], directoryName: string): boolean => {
    return files.some((file) => file.name === directoryName && file.isDirectory)
  },

  openDirectoryPicker: (): Promise<string | null> => {
    api.sendMessage('opening directory')
    return ipcRenderer.invoke('open-directory-picker')
  },

  startDrag: (filename: string): void => {
    ipcRenderer.send('message', 'Starting drag for file: ' + filename)
    ipcRenderer.send('ondragstart', filename)
  },

  doesFileExist: async (filePath: string): Promise<boolean> => {
    try {
      await fs.access(filePath)
      return true
    } catch {
      return false
    }
  },

  search: async (pathParts: string[], query: string): Promise<FileInfo[]> => {
    const directoryPath = path.join(...pathParts)
    const queue: string[] = [directoryPath]
    const results: FileInfo[] = []

    while (queue.length > 0) {
      const currentDir = queue.shift()!
      try {
        const files = await fs.readdir(currentDir, { withFileTypes: true })
        for (const file of files) {
          const fullPath = path.join(currentDir, file.name)
          if (file.name.includes(query)) {
            results.push({
              name: file.name,
              location: currentDir,
              isDirectory: file.isDirectory()
            })
          }
          if (file.isDirectory()) {
            queue.push(fullPath)
          }
        }
      } catch (err) {
        api.sendMessage(`Error searching directory ${currentDir}: ${err}`)
      }
    }

    return results
  },

  getAudioMetadata: async (filePath: string): Promise<IAudioMetadata | null> => {
    try {
      const metadata = await parseFile(filePath)
      return metadata
    } catch (err) {
      ipcRenderer.send('message', 'Error getting audio metadata: ' + err)
      return null
    }
  },

  getKeyBindings: (): Promise<any> => ipcRenderer.invoke('get-key-bindings'),
  saveKeyBindings: (bindings): Promise<any> => ipcRenderer.invoke('save-key-bindings', bindings),

  deleteFile: async (filePath: string): Promise<boolean> => {
    try {
      await fs.unlink(filePath)
      return true
    } catch (err) {
      api.sendMessage(`Error deleting file ${filePath}: ${err}`)
      throw err
    }
  },

  copyFile: async (source: string, destinationPath: string): Promise<boolean> => {
    try {
      await fs.copyFile(source, destinationPath)
      return true
    } catch (err) {
      api.sendMessage(`Error copying file ${source}: ${err}`)
      throw err
    }
  },

  copyFileToClipboard: (filePath: string): Promise<boolean> => {
    return new Promise((resolve, reject) => {
      try {
        api.sendMessage('copied file to clipboard')
        clipboard.writeBuffer('FileNameW', Buffer.from(filePath, 'utf8'))
        resolve(true)
      } catch (err) {
        api.sendMessage(`Error copying file to clipboard: ${err}`)
        reject(err)
      }
    })
  },

  hasFileInClipboard: (): boolean => {
    try {
      return clipboard.has('FileNameW')
    } catch (err) {
      api.sendMessage(`Error checking clipboard: ${err}`)
      return false
    }
  },

  showSaveDialog: (): Promise<string | null> => {
    return ipcRenderer.invoke('show-save-dialog')
  },

  setAlwaysOnTop: (value: boolean): Promise<boolean> =>
    ipcRenderer.invoke('set-always-on-top', value),
  getAlwaysOnTop: (): Promise<boolean> => ipcRenderer.invoke('get-always-on-top'),
  isAbsolute: (pathString: string): boolean => {
    return path.isAbsolute(pathString)
  },
  sep: (): string => {
    return path.sep
  }
}

if (process.contextIsolated) {
  try {
    contextBridge.exposeInMainWorld('electron', electronAPI)
    contextBridge.exposeInMainWorld('api', api)
  } catch (error) {
    console.error(error)
  }
} else {
  // @ts-ignore (define in dts)
  window.electron = electronAPI
  // @ts-ignore (define in dts)
  window.api = api
}
