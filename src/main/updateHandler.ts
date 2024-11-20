import { autoUpdater } from 'electron-updater'
import { ipcMain, BrowserWindow } from 'electron'
import { is } from '@electron-toolkit/utils'

export class UpdateHandler {
  private mainWindow: BrowserWindow
  private updateAvailable: boolean = false

  constructor(mainWindow: BrowserWindow) {
    this.mainWindow = mainWindow
    this.initialize()
  }

  private initialize(): void {
    if (is.dev) {
      // In development, we'll use a local dev server for testing updates
      autoUpdater.updateConfigPath = 'dev-app-update.yml'
    }

    // Configure autoUpdater
    autoUpdater.autoDownload = false
    autoUpdater.autoInstallOnAppQuit = true

    // Handle update events
    autoUpdater.on('checking-for-update', () => {
      this.sendStatusToWindow('checking-for-update')
    })

    autoUpdater.on('update-available', (info) => {
      this.updateAvailable = true
      this.sendStatusToWindow('update-available', info)
    })

    autoUpdater.on('update-not-available', (info) => {
      this.updateAvailable = false
      this.sendStatusToWindow('update-not-available', info)
    })

    autoUpdater.on('error', (err) => {
      this.sendStatusToWindow('error', err)
    })

    autoUpdater.on('download-progress', (progressObj) => {
      this.sendStatusToWindow('download-progress', progressObj)
    })

    autoUpdater.on('update-downloaded', (info) => {
      this.sendStatusToWindow('update-downloaded', info)
    })

    // Handle IPC messages from renderer
    ipcMain.handle('check-for-updates', () => {
      if (!is.dev) {
        autoUpdater.checkForUpdates()
      }
    })

    ipcMain.handle('start-update', async () => {
      if (this.updateAvailable) {
        autoUpdater.downloadUpdate()
      }
    })

    ipcMain.handle('quit-and-install', () => {
      autoUpdater.quitAndInstall()
    })

    // Check for updates immediately
    if (!is.dev) {
      setTimeout(() => {
        autoUpdater.checkForUpdates()
      }, 3000) // Check after 3 seconds to allow the app to load
    }

    // Check for updates every 4 hours
    setInterval(() => {
      if (!is.dev) {
        autoUpdater.checkForUpdates()
      }
    }, 4 * 60 * 60 * 1000)
  }

  private sendStatusToWindow(status: string, data?: any): void {
    this.mainWindow.webContents.send('update-status', { status, data })
  }
}
