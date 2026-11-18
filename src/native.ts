import type { Language } from './locale'

export interface Profile {
  id: string
  name: string
  build?: string | null
  kind?: 'package' | 'legacy'
  bytes?: number | null
  archivedFiles?: number
}

export interface LoaderStatus {
  installed: boolean
  path: string | null
}
export interface View {
  phase: 'unmanaged' | 'awaitingVanilla' | 'ready' | 'recovery'
  activeId: string | null
  profiles: Profile[]
  baselineBuild: string | null
  steamBuild: string | null
  steamIssue?: string | null
  stale: boolean
  gamePath: string
  vaultPath: string
}

export interface Report {
  matches: boolean
  checked: number
  added: string[]
  changed: string[]
  missing: string[]
  stale: boolean
  baselineBuild: string | null
  steamBuild: string | null
}

export interface Backup {
  id: string
  created: number
  files: number
  bytes: number
  label: string
  accounts: string[]
  formats: string[]
  reason: 'manual' | 'beforeRestore' | 'beforeSwitch' | 'periodic'
  pinned?: boolean
}

export interface SaveFile {
  account: string
  name: string
  modified: number
  bytes: number
}

export interface Slot {
  slot: number
  name: string
  level: number
  seconds: number
}

interface TauriCore {
  invoke<T>(command: string, args?: Record<string, unknown>): Promise<T>
}

declare global {
  interface Window {
    __TAURI__?: { core: TauriCore }
  }
}

export function hasDesktop(): boolean {
  return typeof window !== 'undefined' && Boolean(window.__TAURI__?.core?.invoke)
}

const pickerCommands = new Set(['pick_game', 'pick_source', 'pick_native_entry', 'select_loader', 'export_backup', 'import_backup'])

export async function native<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (!hasDesktop()) throw new Error('Windows 데스크톱 앱에서 이용할 수 있습니다.')
  const options = pickerCommands.has(command) ? { ...args, english: document.documentElement.lang === 'en' } : args
  return window.__TAURI__!.core.invoke<T>(command, options)
}

export function activeName(view: View | null, lang: Language = 'ko'): string {
  if (view?.activeId === 'vanilla') return lang === 'en' ? 'Vanilla' : '바닐라'
  return activeProfile(view)?.name ?? (lang === 'en' ? 'Not registered' : '등록 전')
}

export function activeProfile(view: View | null): Profile | undefined {
  return view?.profiles.find(profile => profile.id === view.activeId)
}

export function canLaunchPackage(view: View | null): boolean {
  const profile = activeProfile(view)
  return Boolean(view?.phase === 'ready' && profile?.kind === 'package' && view.steamBuild &&
    !view.steamIssue && !view.stale && (!profile.build || profile.build === view.steamBuild))
}

export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes.toLocaleString('ko-KR')} B`
  const unit = bytes < 1024 ** 2 ? 1 : bytes < 1024 ** 3 ? 2 : 3
  return `${(bytes / 1024 ** unit).toLocaleString('ko-KR', { maximumFractionDigits: 1 })} ${['B', 'KB', 'MB', 'GB'][unit]}`
}


