import { app, shell, BrowserWindow, ipcMain, dialog, protocol, net } from 'electron'
import path, { join } from 'path'
import { electronApp, optimizer, is } from '@electron-toolkit/utils'

// Use dynamic import for electron-store
let store
;(async () => {
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

function getIconPath(): string {
  if (is.dev) {
    return path.join(process.cwd(), 'resources', 'icon.png')
  } else {
    return path.join(__dirname, '../../resources/icon.png')
  }
}

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

protocol.registerSchemesAsPrivileged([
  {
    scheme: 'sample',
    privileges: { bypassCSP: true, stream: true, supportFetchAPI: true }
  }
])

app.whenReady().then(() => {
  protocol.handle('sample', (request) => {
    const filePath = request.url.replace('sample:///', '')
    return net.fetch('file://' + filePath)
  })
})

function createWindow(): void {
  const mainWindow = new BrowserWindow({
    width: 900,
    height: 670,
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
  mainWindow.setAlwaysOnTop(alwaysOnTop)

  mainWindow.once('ready-to-show', () => {
    mainWindow.show()
    mainWindow.focus()
  })

  mainWindow.webContents.setWindowOpenHandler((details) => {
    shell.openExternal(details.url)
    return { action: 'deny' }
  })

  ipcMain.handle('open-directory-picker', async () => {
    console.log('opening directory')
    const result = await dialog.showOpenDialog({
      properties: ['openDirectory'],
      title: 'Select a directory'
    })
    if (result.filePaths[0]) {
      store?.set('lastSelectedDirectory', result.filePaths[0])
    }
    return result.filePaths[0]
  })

  ipcMain.handle('get-last-selected-directory', async () => {
    return store?.get('lastSelectedDirectory')
  })

  if (is.dev && process.env['ELECTRON_RENDERER_URL']) {
    mainWindow.loadURL(process.env['ELECTRON_RENDERER_URL'])
  } else {
    mainWindow.loadFile(join(__dirname, '../renderer/index.html'))
  }

  /* let windowState = store?.get('windowState')

  [('resize', 'move', 'close')].forEach((element) => {
    mainWindow.on(element, () => {
      windowState.isMaximized = mainWindow.isMaximized()

      if (!windowState.isMaximized) {
        windowState.bounds = mainWindow.getBounds()
      }

      store.set('windowState', windowState)
    })
  }) */
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
