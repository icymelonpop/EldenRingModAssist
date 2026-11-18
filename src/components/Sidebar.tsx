import { useEffect, useState } from 'react'
import type { ReactElement } from 'react'
import { TbChevronRight, TbCircleDot } from 'react-icons/tb'
import { activeName } from '@/native'
import type { View } from '@/native'
import CommandPalette from './CommandPalette'
import type { Destination } from './CommandPalette'
import { navigation, pageTitle, titles } from './ringNavigation'
import type { Page } from './ringNavigation'
import { useRingShell } from './RingShell'
import { useLocale } from '@/locale'

export { titles }
export type { Page }

interface Props {
  page: Page
  select: (page: Page) => void
  view: View | null
}

function destinationElement(destination: Destination, view: View | null): HTMLElement | null {
  const workspace = document.getElementById('workspace')
  if (!workspace) return null
  const headings = Array.from(workspace.querySelectorAll<HTMLElement>('h2'))
  if (destination.selector) {
    const target = workspace.querySelector<HTMLElement>(destination.selector)
    if (target && destination.selectTab && target.getAttribute('aria-pressed') === 'false') target.click()
    return target
  }
  if (destination.control) {
    const buttons = Array.from(workspace.querySelectorAll<HTMLButtonElement>('button:not(:disabled)'))
    const control = buttons.find(button => button.textContent?.trim() === destination.control)
    if (control) return control
    if (destination.page === 'journal') return null
  }
  if (destination.profileId && view) {
    const ids = [...(view.phase === 'ready' ? ['vanilla'] : []), ...view.profiles.map(profile => profile.id)]
    const name = workspace.querySelectorAll<HTMLElement>('.rows .row-text > strong')[ids.indexOf(destination.profileId)]
    if (name) return name
  }
  if (destination.heading) {
    const heading = headings.find(element => element.textContent?.trim().split(' · ')[0] === destination.heading)
    if (heading) return heading
  }
  return workspace.querySelector<HTMLElement>('h1') ?? workspace
}

export default function Sidebar({ page, select, view }: Props): ReactElement {
  const { searchOpen, setSearchOpen } = useRingShell()
  const { lang, t } = useLocale()
  const [destination, setDestination] = useState<Destination | null>(null)
  const status = view?.phase === 'ready' ? activeName(view, lang) : view?.phase === 'recovery' ? t('전환 복구 필요', 'Recovery required') :
    view?.phase === 'awaitingVanilla' ? t('바닐라 다운로드 대기', 'Awaiting vanilla files') : view ? t('원본 등록 대기', 'Awaiting original files') : t('설치 연결 대기', 'Game not connected')

  useEffect(() => {
    if (!destination) return
    if (page !== destination.page) { setDestination(null); return }
    let observer: MutationObserver | undefined
    // The page's own mount effects can reset scroll and start loading save files.
    const reveal = (): void => {
      const workspace = document.getElementById('workspace')
      let target = destinationElement(destination, view)
      if (!target && page === 'journal' && workspace) {
        if (workspace.querySelector('[role="alert"]')) target = workspace.querySelector<HTMLElement>('h1')
        else {
          // The journal initializes its notebook after its first render.
          if (!observer) {
            observer = new MutationObserver(reveal)
            observer.observe(workspace, { childList: true, subtree: true })
          }
          return
        }
      }
      observer?.disconnect()
      if (!target) { setDestination(null); return }
      if (target.tabIndex < 0) target.tabIndex = -1
      target.focus({ preventScroll: true })
      target.scrollIntoView({ block: 'nearest' })
      if (page === 'saves' && workspace?.querySelector('.notice.busy')) {
        observer = new MutationObserver(() => {
          if (workspace.querySelector('.notice.busy')) return
          if (document.activeElement === target) target.scrollIntoView({ block: 'nearest' })
          setDestination(null)
        })
        observer.observe(workspace, { childList: true, subtree: true })
      } else setDestination(null)
    }
    const frame = requestAnimationFrame(reveal)
    return () => { cancelAnimationFrame(frame); observer?.disconnect() }
  }, [page, destination, view])

  return (
    <>
      <aside id='ring-navigation' className='sidebar ring-sidebar' data-page={page} aria-label={t('탐색 메뉴', 'Navigation menu')}>
        <nav className='ring-navigation' aria-label={t('주 탐색', 'Main navigation')}>
          {navigation.map(({ page: id, icon: Icon }) => (
            <button type='button' key={id} title={pageTitle(id, lang)} aria-label={pageTitle(id, lang)} aria-current={page === id ? 'page' : undefined}
              className={page === id ? 'nav-item current' : 'nav-item'} onClick={() => select(id)}>
              <Icon size={19} aria-hidden /><span className='nav-label'>{pageTitle(id, lang)}</span><TbChevronRight className='nav-arrow' size={14} aria-hidden />
            </button>
          ))}
        </nav>
        <div className='side-status' title={status}>
          <TbCircleDot size={19} aria-hidden /><div><span>{view?.phase === 'ready' ? t('활성 프로필', 'Active profile') : t('설치 상태', 'Installation status')}</span><strong>{status}</strong></div>
        </div>
      </aside>
      {searchOpen && <CommandPalette view={view} close={() => setSearchOpen(false)} go={next => {
        setSearchOpen(false)
        setDestination(next)
        select(next.page)
      }} />}
    </>
  )
}
