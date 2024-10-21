import './index.css'
import React from 'react'
import ReactDOM from 'react-dom/client'
import { App } from './App'
import { AudioProvider } from './hooks/AudioContextProvider'
import { ThemeProvider } from './context/ThemeContext'

ReactDOM.createRoot(document.getElementById('root') as HTMLElement).render(
  <React.StrictMode>
    <ThemeProvider>
      <AudioProvider>
        <App />
      </AudioProvider>
    </ThemeProvider>
  </React.StrictMode>
)
