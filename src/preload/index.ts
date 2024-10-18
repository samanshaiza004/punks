import { contextBridge } from 'electron'
import { electronAPI } from '@electron-toolkit/preload'
import { ipcRenderer } from 'electron/renderer'
import fs from 'fs'
import path from 'path'
import { FileInfo } from '../renderer/src/types/FileInfo'
import * as musicMetadata from 'music-metadata'

// Custom APIs for renderer

// Use `contextBridge` APIs to expose Electron APIs to
// renderer only if context isolation is enabled, otherwise
// just add to the DOM global.
export const api = {
  sendMessage: (message: string): void => {
    ipcRenderer.send('message', message)
  },

  renderPath: (pathParts: string[]): string => {
    return path.join(...pathParts)
  },

  readDir: (pathParts: string[]): Promise<FileInfo[]> => {
    return new Promise((resolve, reject) => {
      const directoryPath = path.join(...pathParts)
      fs.readdir(directoryPath, { withFileTypes: true }, (err, files) => {
        if (err) {
          const errorMessage = `Error reading directory ${directoryPath}: ${err}`
          api.sendMessage(errorMessage)
          reject(new Error(errorMessage))
        } else {
          const fileInfoList = files.map((file) => ({
            name: file.name,
            location: directoryPath,
            isDirectory: file.isDirectory()
          }))
          resolve(fileInfoList)
        }
      })
    })
  },

  moveDirectory: async (currentPath: string[], newDirectory: string): Promise<string[]> => {
    const directoryIndex = currentPath.indexOf(newDirectory)
    if (directoryIndex !== -1) {
      return currentPath.slice(0, directoryIndex + 1)
    } else {
      try {
        const directoryContents = await api.readDir(currentPath)
        if (api.containsDirectory(directoryContents, newDirectory)) {
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
  isDirectory: (fullPath: string): boolean => {
    try {
      return fs.lstatSync(fullPath).isDirectory()
    } catch (err) {
      api.sendMessage('error checking if path is directory: ' + err)
      return false
    }
  },

  containsDirectory: (files: FileInfo[], directoryName: string): boolean => {
    return files.some((file) => file.name === directoryName && file.isDirectory)
  },

  openDirectoryPicker: (): Promise<string | null> => {
    api.sendMessage('opengin direictory')
    return ipcRenderer.invoke('open-directory-picker')
  },

  startDrag: (filename: string) => {
    ipcRenderer.send('message', 'Starting drag for file: ' + filename)
    ipcRenderer.send('ondragstart', filename)
  },

  doesFileExist: (filePath: string): boolean => {
    try {
      return fs.existsSync(filePath)
    } catch (err) {
      ipcRenderer.send('message', 'Error checking if file exists: ' + err)
      return false
    }
  },

  search: (pathParts: string[], query: string): Promise<FileInfo[]> => {
    const directoryPath = path.join(...pathParts)

    const searchRecursively = (dir: string, query: string): FileInfo[] => {
      let results: FileInfo[] = []
      try {
        const files = fs.readdirSync(dir, { withFileTypes: true })
        files.forEach((file) => {
          const fullPath = path.join(dir, file.name)
          if (file.name.includes(query)) {
            results.push({
              name: file.name,
              location: fullPath,
              isDirectory: file.isDirectory()
            })
          }
          if (file.isDirectory()) {
            results = results.concat(searchRecursively(fullPath, query))
          }
        })
      } catch (err) {
        api.sendMessage(`Error searching directory ${dir}: ${err}`)
      }
      return results
    }

    return new Promise((resolve, reject) => {
      try {
        const results = searchRecursively(directoryPath, query)
        resolve(results)
      } catch (err) {
        reject(err)
      }
    })
  },
  getAudioMetadata: async (filePath: string) => {
    try {
      const metadata = await musicMetadata.parseFile(filePath)

      return metadata
    } catch (err) {
      ipcRenderer.send('message', 'Error getting audio metadata: ' + err)
      return null
    }
  },
  getKeyBindings: () => ipcRenderer.invoke('get-key-bindings'),
  saveKeyBindings: (bindings) => ipcRenderer.invoke('save-key-bindings', bindings)
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
