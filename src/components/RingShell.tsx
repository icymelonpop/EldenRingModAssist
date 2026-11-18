import { createContext, useContext, useEffect, useState } from 'react'
import type { ReactElement, ReactNode } from 'react'

type Rail = 'expanded' | 'collapsed'
export type GlassLevel = 'normal' | 'high' | 'off'
export type ThemeMode = 'dark' | 'light'
const RAIL_KEY = 'elden-ring-mod-assist.theme.rail'
const GLASS_KEY = 'elden-ring-mod-assist.theme.glass'
const MODE_KEY = 'elden-ring-mod-assist.theme.mode'

interface ShellState {
  collapsed: boolean
  toggleRail: () => void
  searchOpen: boolean
  setSearchOpen: (value: boolean) => void
  glass: GlassLevel
  setGlass: (value: GlassLevel) => void
  themeMode: ThemeMode
  setThemeMode: (value: ThemeMode) => void
}

const ShellContext = createContext<ShellState | null>(null)

export function useRingShell(): ShellState {
  const state = useContext(ShellContext)
  if (!state) throw new Error('RingShell provider is missing.')
  return state
}

export default function RingShell({ children }: { children: ReactNode }): ReactElement {
  const [preference, setPreference] = useState<Rail | null>(null)
  const [narrow, setNarrow] = useState(false)
  const [searchOpen, setSearchOpen] = useState(false)
  const [glass, updateGlass] = useState<GlassLevel>('normal')
  const [themeMode, updateThemeMode] = useState<ThemeMode>('dark')
  const collapsed = preference === null ? narrow : preference === 'collapsed'

  useEffect(() => {
    try {
      const stored = localStorage.getItem(RAIL_KEY)
      if (stored === 'expanded' || stored === 'collapsed') setPreference(stored)
      const level = localStorage.getItem(GLASS_KEY)
      if (level === 'normal' || level === 'high' || level === 'off') updateGlass(level)
      const mode = localStorage.getItem(MODE_KEY)
      if (mode === 'light' || mode === 'dark') updateThemeMode(mode)
    } catch { /* Use the defaults when storage is unavailable. */ }
    const media = window.matchMedia('(max-width: 840px)')
    const resize = (): void => setNarrow(media.matches)
    resize()
    media.addEventListener('change', resize)
    return () => media.removeEventListener('change', resize)
  }, [])

  useEffect(() => {
    function shortcut(event: KeyboardEvent): void {
      if (event.defaultPrevented || event.isComposing || event.altKey || document.querySelector('dialog[open]')) return
      const isTyping = event.target instanceof Element && event.target.closest('input, textarea, select, [contenteditable="true"]')
      const commandK = (event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'k'
      const slash = event.key === '/' && !isTyping && !event.ctrlKey && !event.metaKey
      if (!commandK && !slash) return
      event.preventDefault()
      setSearchOpen(true)
    }
    window.addEventListener('keydown', shortcut)
    return () => window.removeEventListener('keydown', shortcut)
  }, [])

  function toggleRail(): void {
    const next = collapsed ? 'expanded' : 'collapsed'
    setPreference(next)
    try { localStorage.setItem(RAIL_KEY, next) } catch { /* The choice still applies to this session. */ }
  }

  function setGlass(value: GlassLevel): void {
    updateGlass(value)
    try { localStorage.setItem(GLASS_KEY, value) } catch { /* The choice still applies to this session. */ }
  }

  function setThemeMode(value: ThemeMode): void {
    updateThemeMode(value)
    try { localStorage.setItem(MODE_KEY, value) } catch { /* The choice still applies to this session. */ }
  }

  return (
    <ShellContext.Provider value={{ collapsed, toggleRail, searchOpen, setSearchOpen, glass, setGlass, themeMode, setThemeMode }}>
      <div className='ring-shell' data-theme-mode={themeMode} data-glass={glass} data-rail={collapsed ? 'collapsed' : 'expanded'}>{children}</div>
    </ShellContext.Provider>
  )
}
