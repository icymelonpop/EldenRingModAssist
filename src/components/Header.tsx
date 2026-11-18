import { useEffect, useState } from 'react'
import type { ReactElement } from 'react'
import { TbLayoutSidebarLeftCollapse, TbLayoutSidebarLeftExpand, TbMoon, TbRefresh, TbSearch, TbSun } from 'react-icons/tb'
import { VscChromeClose, VscChromeMaximize, VscChromeMinimize, VscChromeRestore } from 'react-icons/vsc'
import { hasDesktop, native } from '@/native'
import { pageTitle } from './ringNavigation'
import { useLocale } from '@/locale'
import type { Page } from './ringNavigation'
import { useRingShell } from './RingShell'

interface Props {
  page: Page
  refresh: () => void
  disabled: boolean
}

function WindowControls(): ReactElement | null {
  const { t } = useLocale()
  const [desktop, setDesktop] = useState(false)
  const [maximized, setMaximized] = useState(false)
  const [error, setError] = useState(false)

  useEffect(() => {
    if (!hasDesktop()) return
    setDesktop(true)
    let mounted = true
    let timer: number
    function syncMaximized(): void {
      void native<boolean>('plugin:window|is_maximized').then(value => {
        if (mounted) setMaximized(value)
      }).catch(() => { /* A window can close while the query is in flight. */ })
    }
    function onResize(): void {
      window.clearTimeout(timer)
      timer = window.setTimeout(syncMaximized, 50)
    }
    syncMaximized()
    window.addEventListener('resize', onResize)
    return () => {
      mounted = false
      window.clearTimeout(timer)
      window.removeEventListener('resize', onResize)
    }
  }, [])

  if (!desktop) return null
  async function changeWindow(action: 'minimize' | 'toggle_maximize' | 'close'): Promise<void> {
    setError(false)
    try {
      await native<void>('plugin:window|' + action)
      if (action === 'toggle_maximize') setMaximized(await native<boolean>('plugin:window|is_maximized'))
    } catch {
      setError(true)
    }
  }
  const sizeLabel = maximized ? t('이전 크기로 복원', 'Restore window') : t('창 최대화', 'Maximize window')
  const minimizeLabel = t('창 최소화', 'Minimize window')
  const closeLabel = t('창 닫기', 'Close window')
  const SizeIcon = maximized ? VscChromeRestore : VscChromeMaximize
  return (
    <div className='ring-window-controls' role='group' aria-label={t('창 제어', 'Window controls')}>
      <button className='ring-caption-btn' type='button' aria-label={minimizeLabel} title={minimizeLabel} onClick={() => void changeWindow('minimize')}>
        <VscChromeMinimize aria-hidden />
      </button>
      <button className='ring-caption-btn' type='button' aria-label={sizeLabel} title={sizeLabel} onClick={() => void changeWindow('toggle_maximize')}>
        <SizeIcon aria-hidden />
      </button>
      <button className='ring-caption-btn ring-caption-close' type='button' aria-label={closeLabel} title={closeLabel} onClick={() => void changeWindow('close')}>
        <VscChromeClose aria-hidden />
      </button>
      {error && <span className='ring-caption-error' role='alert'>{t('창 제어 요청을 처리할 수 없습니다.', 'Could not change the window state.')}</span>}
    </div>
  )
}

export default function Header({ page, refresh, disabled }: Props): ReactElement {
  const { collapsed, toggleRail, setSearchOpen, themeMode, setThemeMode } = useRingShell()
  const { lang, t } = useLocale()
  const ToggleIcon = collapsed ? TbLayoutSidebarLeftExpand : TbLayoutSidebarLeftCollapse
  const railLabel = collapsed ? t('탐색 메뉴 펼치기', 'Expand navigation') : t('탐색 메뉴 접기', 'Collapse navigation')
  const themeLabel = themeMode === 'dark' ? t('화이트 모드로 전환', 'Switch to light mode') : t('블랙 모드로 전환', 'Switch to dark mode')
  const searchLabel = t('메뉴 및 프로필 검색', 'Search menus and profiles')
  const ThemeIcon = themeMode === 'dark' ? TbSun : TbMoon

  useEffect(() => {
    if (hasDesktop()) {
      // Keep Windows' acrylic tint in step with the application's palette.
      void native<void>('plugin:window|set_theme', { value: themeMode }).catch(() => {
        /* The in-app palette also works with Windows' fallback background. */
      })
    }
  }, [themeMode])
  useEffect(() => {
    if (hasDesktop()) void native<void>('plugin:window|set_title', { value: t('Elden Ring Mod Assist · 황금나무', 'Elden Ring Mod Assist · Erdtree') }).catch(() => {})
  }, [t])

  return (
    <header className='topbar ring-topbar' data-tauri-drag-region>
      <div className='ring-header-brand' data-tauri-drag-region>
        <button type='button' className='icon-btn ring-rail-toggle' onClick={toggleRail}
          aria-expanded={!collapsed} aria-controls='ring-navigation' aria-label={railLabel} title={railLabel}>
          <ToggleIcon size={20} aria-hidden />
        </button>
        <div className='ring-brand' data-tauri-drag-region aria-label='Elden Ring Mod Assist'>
          <svg className='ring-mark' width='30' height='34' viewBox='0 0 32 36' fill='none' aria-hidden='true'>
            <g stroke='currentColor' strokeWidth='1.3' strokeLinecap='round' strokeLinejoin='round'>
              <path d='M16 2 28.5 17.8 16 33.5 3.5 17.8Z' />
              <path d='M16 6v21m0-6-6.7-6.4m6.7 2.1 5.6-5.5m-5.6 12 4.8-4.5M12 28l4-4 4 4' />
            </g>
          </svg>
          <div><span className='wordmark'>ELDEN RING</span><span className='brand-sub'>MOD ASSIST</span></div>
        </div>
      </div>
      <button type='button' className='ring-search-trigger' onClick={() => setSearchOpen(true)}
        aria-haspopup='dialog' aria-label={searchLabel}>
        <TbSearch size={16} aria-hidden /><span>{searchLabel}</span><kbd aria-hidden>Ctrl K</kbd>
      </button>
      <div className='ring-header-tools' data-tauri-drag-region>
        <span className='ring-current-page' data-tauri-drag-region>{pageTitle(page, lang)}</span>
        <button type='button' className='action quiet small' onClick={refresh} disabled={disabled}
          aria-label={t('새로고침', 'Refresh')} title={t('새로고침', 'Refresh')}><TbRefresh size={16} aria-hidden /><span>{t('새로고침', 'Refresh')}</span></button>
        <button className='ring-theme-switch' type='button' aria-label={themeLabel} title={themeLabel}
          onClick={() => setThemeMode(themeMode === 'dark' ? 'light' : 'dark')}>
          <ThemeIcon size={18} aria-hidden />
        </button>
        <WindowControls />
      </div>
    </header>
  )
}
