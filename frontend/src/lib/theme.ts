import { createSignal, createEffect, type Accessor } from 'solid-js'

type Theme = 'light' | 'dark'

const STORAGE_KEY = 'yiliao-theme'

function getSystemTheme(): Theme {
  if (typeof window === 'undefined') return 'light'
  return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light'
}

function getStoredTheme(): Theme | null {
  if (typeof window === 'undefined') return null
  const stored = localStorage.getItem(STORAGE_KEY)
  if (stored === 'light' || stored === 'dark') return stored
  return null
}

function applyTheme(theme: Theme) {
  document.documentElement.classList.toggle('dark', theme === 'dark')
}

const [theme, setThemeSignal] = createSignal<Theme>(getStoredTheme() ?? getSystemTheme())

// Single module-level effect: applies theme and persists to localStorage
// whenever the signal changes. This avoids registering duplicate effects
// when multiple components call useTheme().
if (typeof window !== 'undefined') {
  applyTheme(theme())
  createEffect(() => {
    const t = theme()
    applyTheme(t)
    localStorage.setItem(STORAGE_KEY, t)
  })
}

const toggleTheme = () => {
  setThemeSignal(prev => (prev === 'light' ? 'dark' : 'light'))
}

const setTheme = (t: Theme) => {
  setThemeSignal(t)
}

export function useTheme(): {
  theme: Accessor<Theme>
  toggleTheme: () => void
  setTheme: (t: Theme) => void
} {
  return { theme, toggleTheme, setTheme }
}
