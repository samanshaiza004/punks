// src/main/TagEngineIPC.ts
import { ipcMain, BrowserWindow } from 'electron'
import TagEngine from './TagEngine'
import { IPC_CHANNELS, TagSearchOptions } from '../types'
import * as path from 'path'

export function setupTagEngineIPC(tagEngine: TagEngine): void {
  // Scan directory
  ipcMain.handle(IPC_CHANNELS.SCAN_DIRECTORY, async (_, directoryPath: string) => {
    return await tagEngine.scanDirectory(directoryPath)
  })

  // Search by tags
  ipcMain.handle(
    IPC_CHANNELS.SEARCH_BY_TAGS,
    async (_, tags: string[], options: TagSearchOptions = {}) => {
      return await tagEngine.searchByTags(tags, options)
    }
  )

  // Get directory contents
  ipcMain.handle(IPC_CHANNELS.GET_DIRECTORY_CONTENTS, async (_, directoryPath: string[]) => {
    return await tagEngine.getDirectoryContents(directoryPath)
  })

  // Get parent directory
  ipcMain.handle(IPC_CHANNELS.GET_PARENT_DIRECTORY, async (_, currentPath: string) => {
    return await tagEngine.getParentDirectory(currentPath)
  })

  // Add tag to file
  ipcMain.handle(IPC_CHANNELS.ADD_TAG, async (_, filePath: string, tag: string) => {
    return await tagEngine.addTag(filePath, tag)
  })

  // Remove tag from file
  ipcMain.handle(IPC_CHANNELS.REMOVE_TAG, async (_, filePath: string, tag: string) => {
    return await tagEngine.removeTag()
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
    BrowserWindow.getAllWindows().forEach((window) => {
      window.webContents.send(IPC_CHANNELS.ON_SCAN_PROGRESS, progress)
    })
  })

  // Forward scan complete event
  tagEngine.on('scanComplete', (stats) => {
    BrowserWindow.getAllWindows().forEach((window) => {
      window.webContents.send(IPC_CHANNELS.ON_SCAN_COMPLETE, stats)
    })
  })

  // Forward scan error event
  tagEngine.on('error', (error) => {
    BrowserWindow.getAllWindows().forEach((window) => {
      window.webContents.send(IPC_CHANNELS.ON_SCAN_ERROR, error)
    })
  })
}
