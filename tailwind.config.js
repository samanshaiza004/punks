/** @type {import('tailwindcss').Config} */
module.exports = {
  content: ['./src/**/*.{js,jsx,ts,tsx}'],
  darkMode: 'class',
  safelist: [
    'grid-cols-1',
    'grid-cols-2',
    'grid-cols-3',
    'grid-cols-4',
    'grid-cols-5'
  ],
  theme: {
    extend: {
      keyframes: {
        overlayShow: {
          from: { opacity: 0 },
          to: { opacity: 1 }
        },
        contentShow: {
          from: { opacity: 0, transform: 'translate(-50%, -48%) scale(0.96)' },
          to: { opacity: 1, transform: 'translate(-50%, -50%) scale(1)' }
        },
        slideIn: {
          from: { transform: 'translateX(calc(100% + 1rem))' },
          to: { transform: 'translateX(0)' }
        },
        hide: {
          from: { opacity: 1 },
          to: { opacity: 0 }
        },
        swipeOut: {
          from: { transform: 'translateX(var(--radix-toast-swipe-end-x))' },
          to: { transform: 'translateX(calc(100% + 1rem))' }
        }
      },
      animation: {
        overlayShow: 'overlayShow 150ms cubic-bezier(0.16, 1, 0.3, 1)',
        contentShow: 'contentShow 150ms cubic-bezier(0.16, 1, 0.3, 1)',
        slideIn: 'slideIn 150ms cubic-bezier(0.16, 1, 0.3, 1)',
        hide: 'hide 100ms ease-in',
        swipeOut: 'swipeOut 100ms ease-out'
      }
    }
  },
  plugins: []
}
