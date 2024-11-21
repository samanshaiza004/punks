// src/context/ToastContext.tsx
import * as Toast from '@radix-ui/react-toast'
import { createContext, useContext, ReactNode, useState, useCallback } from 'react'

interface ToastContextType {
  showToast: (message: string, type: 'success' | 'error' | 'info') => void
}

const ToastContext = createContext<ToastContextType | undefined>(undefined)

export const useToast = () => {
  const context = useContext(ToastContext)
  if (!context) {
    throw new Error('useToast must be used within a ToastProvider')
  }
  return context
}

export const ToastProvider = ({ children }: { children: ReactNode }) => {
  const [toasts, setToasts] = useState<Array<{
    id: number
    message: string
    type: 'success' | 'error' | 'info'
  }>>([])

  const showToast = useCallback((message: string, type: 'success' | 'error' | 'info') => {
    const id = Date.now()
    setToasts((prevToasts) => [...prevToasts, { id, message, type }])
  }, [])

  return (
    <Toast.Provider swipeDirection="right">
      <ToastContext.Provider value={{ showToast }}>
        {children}
        {toasts.map(({ id, message, type }) => (
          <Toast.Root
            key={id}
            className={`
              rounded-lg shadow-lg p-4 
              ${type === 'success' ? 'bg-green-500' : type === 'error' ? 'bg-red-500' : 'bg-blue-500'}
              text-white
              data-[state=open]:animate-slideIn
              data-[state=closed]:animate-hide
              data-[swipe=move]:translate-x-[var(--radix-toast-swipe-move-x)]
              data-[swipe=cancel]:translate-x-0
              data-[swipe=cancel]:transition-[transform_200ms_ease-out]
              data-[swipe=end]:animate-swipeOut
            `}
            onOpenChange={(open) => {
              if (!open) {
                setTimeout(() => {
                  setToasts((prevToasts) => prevToasts.filter((t) => t.id !== id))
                }, 100)
              }
            }}
            duration={5000}
          >
            <Toast.Description>{message}</Toast.Description>
          </Toast.Root>
        ))}
        <Toast.Viewport className="fixed top-4 right-4 flex flex-col gap-2 w-[390px] max-w-[100vw] m-0 list-none z-[2147483647] outline-none" />
      </ToastContext.Provider>
    </Toast.Provider>
  )
}