// src/components/ToastContainer.tsx
import React from 'react'
import { Toast } from '../context/ToastContext'

interface ToastContainerProps {
  toasts: Toast[]
}

const ToastContainer: React.FC<ToastContainerProps> = ({ toasts }) => {
  return (
    <div className="fixed top-4 right-4 space-y-2">
      {toasts.map((toast) => (
        <div
          key={toast.id}
          className={`p-3 rounded-lg shadow-lg text-white ${
            toast.type === 'success'
              ? 'bg-green-500'
              : toast.type === 'error'
                ? 'bg-red-500'
                : 'bg-blue-500'
          }`}
        >
          {toast.message}
        </div>
      ))}
    </div>
  )
}

export default ToastContainer
