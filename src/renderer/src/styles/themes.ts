export interface ThemeColors {
  background: string
  foreground: string
  primary: string
  secondary: string
  accent: string
  border: string
  muted: string
}

export interface Theme {
  name: string
  colors: ThemeColors
  dark: boolean
}

export const defaultThemes: Record<string, Theme> = {
  light: {
    name: 'Light',
    dark: false,
    colors: {
      background: '#ffffff',
      foreground: '#1a1a1a',
      primary: '#2563eb',
      secondary: '#64748b',
      accent: '#3b82f6',
      border: '#e2e8f0',
      muted: '#f1f5f9'
    }
  },
  dark: {
    name: 'Dark',
    dark: true,
    colors: {
      background: '#1a1a1a',
      foreground: '#ffffff',
      primary: '#3b82f6',
      secondary: '#94a3b8',
      accent: '#60a5fa',
      border: '#334155',
      muted: '#1e293b'
    }
  },
  obsidian: {
    name: 'Obsidian',
    dark: true,
    colors: {
      background: '#191919',
      foreground: '#dcddde',
      primary: '#7b68ee',
      secondary: '#a39e9e',
      accent: '#9580ff',
      border: '#363636',
      muted: '#252525'
    }
  },
  vscode: {
    name: 'VS Code',
    dark: true,
    colors: {
      background: '#1e1e1e',
      foreground: '#d4d4d4',
      primary: '#0078d4',
      secondary: '#858585',
      accent: '#007acc',
      border: '#404040',
      muted: '#252526'
    }
  }
}

export type ThemeName = keyof typeof defaultThemes

export const generateThemeVariables = (theme: Theme): string => {
  return `
    :root {
      --background: ${theme.colors.background};
      --foreground: ${theme.colors.foreground};
      --primary: ${theme.colors.primary};
      --secondary: ${theme.colors.secondary};
      --accent: ${theme.colors.accent};
      --border: ${theme.colors.border};
      --muted: ${theme.colors.muted};
    }
  `
}
