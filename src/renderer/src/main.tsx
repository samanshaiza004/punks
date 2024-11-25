import './index.css'
import React from 'react'
import ReactDOM from 'react-dom/client'
import { App } from './App'
import { AudioProvider } from './context/AudioContextProvider'
import { ThemeProvider } from './context/ThemeContext'
import { TabProvider } from './context/TabContext'
import { ToastProvider } from './context/ToastContext'
import { ScanProgressProvider } from './context/ScanProgressContext'

ReactDOM.createRoot(document.getElementById('root') as HTMLElement).render(
  <React.StrictMode>
    <TabProvider>
      <ThemeProvider>
      <ScanProgressProvider>
          <AudioProvider>
            <ToastProvider>
              <App />
            </ToastProvider>
          </AudioProvider>
        </ScanProgressProvider>
      </ThemeProvider>
    </TabProvider>
  </React.StrictMode>
)
