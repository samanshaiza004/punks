// src/preload/index.ts

/* eslint-disable @typescript-eslint/no-explicit-any */
import { clipboard, contextBridge } from 'electron'
import { electronAPI } from '@electron-toolkit/preload'
import { ipcRenderer } from 'electron/renderer'
import fs from 'fs/promises'
import fs2 from 'fs'
import path from 'path'
import { FileNode, AudioFile, TagSearchOptions, IPC_CHANNELS } from '../types/index'
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
    files: FileNode[]
    total: number
    hasMore: boolean
  }> => {
    try {
      const directoryPath = path.join(...pathParts)
      const allFiles = await fs.readdir(directoryPath, { withFileTypes: true })
      const total = allFiles.length

      const filesToProcess = limit ? allFiles.slice(offset, offset + limit) : allFiles

      const FileNodeList = filesToProcess.map((file) => ({
        name: file.name,
        path: path.join(directoryPath, file.name),
        type: 'file' as const,
        directory_path: directoryPath,
        last_modified: fs2.statSync(path.join(directoryPath, file.name)).mtimeMs,
        hash: '', // You might want to generate a hash for the file
        tags: [], // Default to empty tags
        id: 0, // You might want to generate a unique ID
        isDirectory: file.isDirectory()
      }))

      return {
        files: FileNodeList,
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
    return ipcRenderer.invoke('get-last-directory')
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

  containsDirectory: (files: FileNode[], directoryName: string): boolean => {
    return files.some((file) => file.name === directoryName)
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

  search: async (pathParts: string[], query: string): Promise<FileNode[]> => {
    const directoryPath = path.join(...pathParts)
    const queue: string[] = [directoryPath]
    const results: FileNode[] = []

    while (queue.length > 0) {
      const currentDir = queue.shift()!
      try {
        const files = await fs.readdir(currentDir, { withFileTypes: true })
        for (const file of files) {
          const fullPath = path.join(currentDir, file.name)
          if (file.name.includes(query)) {
            results.push({
              name: file.name,
              path: currentDir,
              type: 'file' as const,
              directory_path: directoryPath,
              last_modified: fs2.statSync(fullPath).mtimeMs,
              hash: '', // You might want to generate a hash for the file
              tags: [], // Default to empty tags
              id: 0 // You might want to generate a unique ID
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
  saveAutoPlay: (value: boolean): Promise<boolean> => ipcRenderer.invoke('save-auto-play', value),
  getAutoPlay: (): Promise<boolean> => ipcRenderer.invoke('get-auto-play'),
  isAbsolute: (pathString: string): boolean => {
    return path.isAbsolute(pathString)
  },
  sep: (): string => {
    return path.sep
  },

  // Directory navigation
  getDirectoryContents: async (
    directoryPath: string[]
  ): Promise<{
    directories: Array<{ path: string; name: string; lastModified: number; type: 'directory' }>
    files: any[]
    currentPath: string
  }> => {
    return await ipcRenderer.invoke(IPC_CHANNELS.GET_DIRECTORY_CONTENTS, directoryPath)
  },

  getParentDirectory: async (currentPath: string): Promise<string | null> => {
    return await ipcRenderer.invoke(IPC_CHANNELS.GET_PARENT_DIRECTORY, currentPath)
  },

  // TagEngine functionality
  // Stats and initialization
  getTagStats: async (): Promise<{
    totalFiles: number
    totalTags: number
    tagCounts: { name: string; count: number }[]
  }> => {
    return await ipcRenderer.invoke('tag-engine:get-stats')
  },

  // File scanning and metadata
  scanDirectory: async (directory: string): Promise<boolean> => {
    return await ipcRenderer.invoke('scan-directory', directory)
  },

  verifyDirectoryAccess: async (directory: string): Promise<boolean> => {
    return await ipcRenderer.invoke('verify-directory-access', directory)
  },

  onScanProgress: (
    callback: (progress: { total: number; processed: number; percentComplete: number }) => void
  ): void => {
    ipcRenderer.on('scan-progress', (_event, progress) => callback(progress))
  },

  onScanComplete: (callback: () => void): void => {
    ipcRenderer.on('scan-complete', () => callback())
  },

  onScanError: (callback: (error: string) => void): void => {
    ipcRenderer.on('scan-error', (_event, error) => callback(error))
  },

  getFileMetadata: async (filePath: string): Promise<AudioFile | null> => {
    return await ipcRenderer.invoke('tag-engine:get-file-metadata', filePath)
  },

  // Tag management
  addTags: async (tags: string[]): Promise<void> => {
    await ipcRenderer.invoke('tag-engine:add-tags', tags)
  },

  tagFiles: async (files: string[], tag: string): Promise<void> => {
    await ipcRenderer.invoke('tag-engine:tag-files', files, tag)
  },

  untagFile: async (file: string, tag: string): Promise<void> => {
    await ipcRenderer.invoke('tag-engine:untag-file', file, tag)
  },

  // Search operations
  searchByTags: async (tags: string[], options: TagSearchOptions = {}): Promise<AudioFile[]> => {
    return await ipcRenderer.invoke('tag-engine:search-by-tags', tags, options)
  },

  // Event listeners
  onFileAdded: (callback: (file: AudioFile) => void): void => {
    ipcRenderer.on('tag-engine:file-added', (_event, file) => callback(file))
  },

  onFileChanged: (callback: (file: AudioFile) => void): void => {
    ipcRenderer.on('tag-engine:file-changed', (_event, file) => callback(file))
  },

  onFileRemoved: (callback: (path: string) => void): void => {
    ipcRenderer.on('tag-engine:file-removed', (_event, path) => callback(path))
  }
}

// Context bridge setup
if (process.contextIsolated) {
  try {
    contextBridge.exposeInMainWorld('electron', electronAPI)
    contextBridge.exposeInMainWorld('api', {
      ...api,
      getDirectoryContents: api.getDirectoryContents,
      getParentDirectory: api.getParentDirectory,
      getTagStats: api.getTagStats,
      scanDirectory: api.scanDirectory,
      verifyDirectoryAccess: api.verifyDirectoryAccess,
      saveAutoPlay: api.saveAutoPlay,
      getAutoPlay: api.getAutoPlay,
      onScanProgress: api.onScanProgress,
      onScanComplete: api.onScanComplete,
      onScanError: api.onScanError
    })
  } catch (error) {
    console.error('Error setting up context bridge:', error)
  }
} else {
  // @ts-ignore (define in dts)
  window.electron = electronAPI
  // @ts-ignore (define in dts)
  window.api = api
}
