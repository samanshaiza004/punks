// src/main/TagEngineIPC.ts
import { ipcMain } from 'electron'
import TagEngine from './TagEngine'
import { IPC_CHANNELS } from '../renderer/src/types'

export function setupTagEngineIPC(tagEngine: TagEngine): void {
  // Scan directory
  ipcMain.handle(IPC_CHANNELS.SCAN_DIRECTORY, async (_, directoryPath: string) => {
    return await tagEngine.scanDirectory(directoryPath)
  })

  // Get directory contents
  ipcMain.handle(IPC_CHANNELS.GET_DIRECTORY_CONTENTS, async (_, directoryPath: string) => {
    return await tagEngine.getDirectoryContents(directoryPath)
  })

  // Get parent directory
  ipcMain.handle(IPC_CHANNELS.GET_PARENT_DIRECTORY, async (_, currentPath: string) => {
    return await tagEngine.getParentDirectory(currentPath)
  })

  // Search by tags
  ipcMain.handle(IPC_CHANNELS.SEARCH_BY_TAGS, async (_, tags: string[], options = {}) => {
    return await tagEngine.searchByTags(tags, options)
  })

  // Add tag to file
  ipcMain.handle(IPC_CHANNELS.ADD_TAG, async (_, filePath: string, tag: string) => {
    return await tagEngine.addTag(filePath, tag)
  })

  // Remove tag from file
  ipcMain.handle(IPC_CHANNELS.REMOVE_TAG, async (_, filePath: string, tag: string) => {
    return await tagEngine.removeTag(filePath, tag)
  })

  // Get all tags
  ipcMain.handle(IPC_CHANNELS.GET_ALL_TAGS, async () => {
    return await tagEngine.getAllTags()
  })

  // Get stats
  ipcMain.handle(IPC_CHANNELS.GET_STATS, async () => {
    return await tagEngine.getStats()
  })

  // Forward progress events
  tagEngine.on('scanProgress', (progress) => {
    ipcMain.emit(IPC_CHANNELS.SCAN_PROGRESS, progress)
  })

  // Forward completion events
  tagEngine.on('scanComplete', (result) => {
    ipcMain.emit(IPC_CHANNELS.SCAN_COMPLETE, result)
  })

  // Forward error events
  tagEngine.on('error', (error) => {
    ipcMain.emit(IPC_CHANNELS.ERROR, error)
  })
}
