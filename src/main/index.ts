// src/main/index.ts

import { app, shell, BrowserWindow, ipcMain, dialog, protocol, net } from 'electron'
import path, { join } from 'path'
import { electronApp, optimizer, is } from '@electron-toolkit/utils'
import { existsSync } from 'fs'
import TagEngine from './TagEngine'
import { setupTagEngineIPC } from './TagEngineIPC'
import fs from 'fs'
let tagEngine: TagEngine
let mainWindow: BrowserWindow | null = null

// this is a dynamic import silly
let store
;(async (): Promise<void> => {
  const Store = (await import('electron-store')).default
  store = new Store()
})()

ipcMain.handle('get-key-bindings', () => {
  return store.get('keyBindings')
})

ipcMain.handle('save-key-bindings', (_, bindings) => {
  store.set('keyBindings', bindings)
  return true
})

ipcMain.handle('save-last-directory', async (_, directory: string) => {
  try {
    await store.set('lastDirectory', directory)
    return true
  } catch (error) {
    console.error('Error saving last directory:', error)
    return false
  }
})

ipcMain.handle('get-last-directory', async () => {
  try {
    return await store.get('lastDirectory')
  } catch (error) {
    console.error('Error getting last directory:', error)
    return null
  }
})

ipcMain.handle('show-save-dialog', async () => {
  const result = await dialog.showSaveDialog({
    properties: ['createDirectory']
  })
  return result.canceled ? null : result.filePath
})

ipcMain.handle('set-always-on-top', (_, value: boolean) => {
  const windows = BrowserWindow.getAllWindows()
  windows.forEach((window) => window.setAlwaysOnTop(value))
  store.set('alwaysOnTop', value)
  return true
})

ipcMain.handle('get-always-on-top', () => {
  return store.get('alwaysOnTop', false)
})

protocol.registerSchemesAsPrivileged([
  {
    scheme: 'sample',
    privileges: { bypassCSP: true, stream: true, supportFetchAPI: true }
  }
])

app.whenReady().then(() => {
  tagEngine = new TagEngine()
  setupTagEngineIPC(tagEngine)

  console.log('TagEngine initialized:', !!tagEngine)

  // Add handler for directory access verification
  ipcMain.handle('verify-directory-access', async (_event, directoryPath: string) => {
    try {
      if (!directoryPath || !existsSync(directoryPath)) {
        throw new Error('Invalid directory path')
      }

      await fs.promises.access(directoryPath, fs.constants.R_OK)
      return true
    } catch (error: any) {
      console.error('Directory access verification failed:', error)
      throw new Error(error.message || 'Cannot access directory')
    }
  })

  // In main/index.ts, modify the scan-directory handler:
  ipcMain.handle('scan-directory', async (_event, directoryPath: string) => {
    try {
      if (!directoryPath || !existsSync(directoryPath)) {
        throw new Error('Invalid directory path')
      }

      if (!tagEngine) {
        throw new Error('TagEngine not initialized')
      }

      // Add more detailed error handling
      const canAccess = await fs.promises.access(directoryPath, fs.constants.R_OK)
        .then(() => true)
        .catch(() => false)
      
      if (!canAccess) {
        throw new Error('Cannot access directory: Permission denied')
      }

      // Start scanning in the background
      await tagEngine.scanDirectory(directoryPath)
        .then(() => {
          mainWindow?.webContents.send('scan-complete')
          return true
        })
        .catch((error) => {
          console.error('Error during directory scan:', error)
          mainWindow?.webContents.send('scan-error', error.message || 'Failed to scan directory')
          return false
        })

      return true
    } catch (error: any) {
      console.error('Error during directory scan:', error)
      mainWindow?.webContents.send('scan-error', error.message || 'Failed to scan directory')
      return false
    }
  })

  // Add event listeners for TagEngine events
  tagEngine.on('scanProgress', (progress) => {
    if (mainWindow?.isDestroyed()) return
    mainWindow?.webContents.send('scan-progress', progress)
  })

  tagEngine.on('scanComplete', (stats) => {
    if (mainWindow?.isDestroyed()) return
    mainWindow?.webContents.send('scan-complete', stats)
  })

  tagEngine.on('error', (error) => {
    if (mainWindow?.isDestroyed()) return
    mainWindow?.webContents.send('scan-error', error.message || 'Unknown error occurred')
  })

  protocol.handle('sample', async (request) => {
    try {
      const rawPath = request.url.replace('sample:///', '')

      const decodedPath = rawPath
        .replace(/%23/g, '#') // Handle # symbol
        .replace(/%25/g, '%') // Handle % symbol
        .replace(/%20/g, ' ') // Handle spaces
        .replace(/%26/g, '&') // Handle & symbol
        .replace(/%3F/g, '?') // Handle ? symbol
        .replace(/%2B/g, '+') // Handle + symbol
        .replace(/%5B/g, '[') // Handle [ symbol
        .replace(/%5D/g, ']') // Handle ] symbol
        .replace(/%40/g, '@') // Handle @ symbol
        .replace(/%21/g, '!') // Handle ! symbol
        .replace(/%24/g, '$') // Handle $ symbol
        .replace(/%5C/g, '\\') // Handle backslashes
        .replace(/%3A/g, ':') // Handle colons

      // Normalize the path for the current platform
      const normalizedPath = path.normalize(decodedPath)

      // Verify the file exists
      if (!existsSync(normalizedPath)) {
        console.error('File does not exist:', normalizedPath)
        throw new Error('File not found')
      }

      const encodeSpecialChars = (str: string): string => {
        return str
          .replace(/#/g, '%23')
          .replace(/\s/g, '%20')
          .replace(/\(/g, '%28')
          .replace(/\)/g, '%29')
          .replace(/!/g, '%21')
          .replace(/\[/g, '%5B')
          .replace(/\]/g, '%5D')
          .replace(/@/g, '%40')
          .replace(/\$/g, '%24')
          .replace(/&/g, '%26')
          .replace(/\+/g, '%2B')
          .replace(/'/g, '%27')
          .replace(/,/g, '%2C')
          .replace(/;/g, '%3B')
          .replace(/=/g, '%3D')
          .replace(/\?/g, '%3F')
      }

      let fileUrl: string
      if (process.platform === 'win32') {
        const forwardSlashPath = normalizedPath.replace(/\\/g, '/')
        const [drive, ...pathParts] = forwardSlashPath.split(':')
        const encodedPath = pathParts
          .join(':')
          .split('/')
          .map((segment) => encodeSpecialChars(segment))
          .join('/')
        fileUrl = `file:///${drive}:${encodedPath}`
      } else {
        // Unix path handling
        const segments = normalizedPath.split('/')
        const encodedPath = segments.map((segment) => encodeSpecialChars(segment)).join('/')
        fileUrl = `file://${encodedPath}`
      }

      // Debug logging
      console.log('Original URL:', request.url)
      console.log('Final file URL:', fileUrl)
      console.log('File exists check:', existsSync(normalizedPath))

      const response = await net.fetch(fileUrl)

      if (!response.ok) {
        throw new Error(`Failed to fetch file: ${response.status} ${response.statusText}`)
      }

      return response
    } catch (error) {
      console.error('Protocol handler error:', error, 'for URL:', request.url)
      throw error
    }
  })
})

function createWindow(): void {
  mainWindow = new BrowserWindow({
    width: 900,
    height: 670,
    minWidth: 480,
    minHeight: 400,
    show: false,
    autoHideMenuBar: true,
    backgroundColor: '#1f2937',
    webPreferences: {
      contextIsolation: true,
      nodeIntegration: true,
      preload: join(__dirname, '../preload/index.js'),
      sandbox: false
    }
  })

  const alwaysOnTop = store?.get('alwaysOnTop', false) ?? false
  mainWindow?.setAlwaysOnTop(alwaysOnTop)

  mainWindow?.once('ready-to-show', () => {
    mainWindow?.show()
    mainWindow?.focus()
  })

  mainWindow?.webContents?.setWindowOpenHandler((details) => {
    shell.openExternal(details.url)
    return { action: 'deny' }
  })

  ipcMain.handle('open-directory-picker', async () => {
    const result = await dialog.showOpenDialog({
      properties: ['openDirectory']
    })
    if (!result.canceled) {
      store?.set('lastDirectory', result.filePaths[0])
    }
    return result.filePaths[0]
  })

  

  if (is.dev && process.env['ELECTRON_RENDERER_URL']) {
    mainWindow.loadURL(process.env['ELECTRON_RENDERER_URL'])
  } else {
    mainWindow.loadFile(join(__dirname, '../renderer/index.html'))
  }
}

ipcMain.on('ondragstart', (event, filePath) => {
  const absolutePath = path.resolve(filePath)
  const iconPath = getIconPath()

  try {
    if (require('fs').existsSync(iconPath)) {
      event.sender.startDrag({
        file: absolutePath,
        icon: iconPath
      })
    } else {
      console.error(`Icon not found at path: ${iconPath}`)
    }
  } catch (error) {
    console.error('Error starting drag:', error)
  }
})

app.whenReady().then(() => {
  electronApp.setAppUserModelId('com.electron')

  app.on('browser-window-created', (_, window) => {
    optimizer.watchWindowShortcuts(window)
  })

  ipcMain.on('message', (_, message) => {
    console.log(message)
  })

  createWindow()

  app.on('activate', function () {
    if (BrowserWindow.getAllWindows().length === 0) createWindow()
  })
})

app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') {
    app.quit()
  }
})

function getIconPath(): string {
  if (is.dev) {
    return path.join(process.cwd(), 'resources', 'icon.png')
  } else {
    return path.join(__dirname, '../../resources/icon.png')
  }
}
