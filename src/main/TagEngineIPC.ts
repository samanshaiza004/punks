// src/main/TagEngineIPC.ts
import { ipcMain, IpcMainInvokeEvent } from 'electron'
import TagEngine from './TagEngine'
import { TagSearchOptions } from './types'

export function setupTagEngineIPC(tagEngine: TagEngine): void {
  // File scanning and metadata
  ipcMain.handle(
    'tag-engine:scan-directory',
    async (_event: IpcMainInvokeEvent, directoryPath: string) => {
      return tagEngine.watchDirectory(directoryPath)
    }
  )

  ipcMain.handle(
    'tag-engine:get-file-metadata',
    async (_event: IpcMainInvokeEvent, filePath: string) => {
      return await tagEngine.getFileMetadata(filePath)
    }
  )

  // Tag management
  ipcMain.handle('tag-engine:add-tags', async (_event: IpcMainInvokeEvent, tags: string[]) => {
    return await tagEngine.addTags(tags)
  })

  ipcMain.handle(
    'tag-engine:tag-files',
    async (_event: IpcMainInvokeEvent, files: string[], tag: string) => {
      return await tagEngine.tagFiles(files, tag)
    }
  )

  ipcMain.handle(
    'tag-engine:untag-file',
    async (_event: IpcMainInvokeEvent, file: string, tag: string) => {
      return await tagEngine.untagFile(file, tag)
    }
  )

  // Search operations
  ipcMain.handle(
    'tag-engine:search-by-tags',
    async (_event: IpcMainInvokeEvent, tags: string[], options: TagSearchOptions) => {
      return await tagEngine.searchByTags(tags, options)
    }
  )

  // Statistics
  ipcMain.handle('tag-engine:get-stats', async () => {
    return await tagEngine.getStats()
  })

  // Cleanup on app quit
  ipcMain.handle('tag-engine:close', async () => {
    return await tagEngine.close()
  })
}
