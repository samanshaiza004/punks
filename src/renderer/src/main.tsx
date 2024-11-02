import './index.css'
import React from 'react'
import ReactDOM from 'react-dom/client'
import { App } from './App'
import { AudioProvider } from './context/AudioContextProvider'
import { ThemeProvider } from './context/ThemeContext'
import { TabProvider } from './context/TabContext'
import { ToastProvider } from './context/ToastContext'
import { ContextMenuProvider } from './context/ContextMenuContext'

ReactDOM.createRoot(document.getElementById('root') as HTMLElement).render(
  <React.StrictMode>
    <TabProvider>
      <ThemeProvider>
        <ContextMenuProvider>
          <AudioProvider>
            <ToastProvider>
              <App />
            </ToastProvider>
          </AudioProvider>
        </ContextMenuProvider>
      </ThemeProvider>
    </TabProvider>
  </React.StrictMode>
)
