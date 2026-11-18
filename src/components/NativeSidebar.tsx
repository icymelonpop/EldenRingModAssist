import type { ReactElement } from 'react'
import {
  TbArchive, TbChevronRight, TbCircleDot, TbFolders,
  TbLayoutDashboard, TbSettings, TbSword, TbNotebook, TbCalculator,
} from 'react-icons/tb'
import { activeName } from '@/native'
import type { View } from '@/native'

const items = [
  { id: 'overview', text: '사용 현황', icon: TbLayoutDashboard },
  { id: 'profiles', text: '모드 프로필', icon: TbFolders },
  { id: 'saves', text: '세이브 관리', icon: TbArchive },
  { id: 'journal', text: '모험 수첩', icon: TbNotebook },
  { id: 'planner', text: '빌드 계산기', icon: TbCalculator },
  { id: 'settings', text: '설치 설정', icon: TbSettings },
] as const
export type Page = (typeof items)[number]['id']
export const titles: Record<Page, string> = {
  overview: '사용 현황', profiles: '모드 프로필', saves: '세이브 관리', journal: '모험 수첩', planner: '빌드 계산기', settings: '설치 설정',
}

interface Props {
  page: Page
  select: (page: Page) => void
  view: View | null
}

export default function Sidebar({ page, select, view }: Props): ReactElement {
  return (
    <aside className='sidebar'>
      <div className='brand'>
        <span className='crest'><TbSword aria-hidden size={24} /></span>
        <div><span className='wordmark'>ELDEN RING</span><span className='brand-sub'>MOD ASSIST</span></div>
      </div>
      <nav aria-label='주 탐색'>{items.map(({ id, text, icon: Icon }) => (
        <button type='button' title={text} key={id} aria-current={page === id ? 'page' : undefined}
          className={page === id ? 'nav-item current' : 'nav-item'} onClick={() => select(id)}>
          <Icon size={19} aria-hidden /><span>{text}</span><TbChevronRight className='nav-arrow' size={14} aria-hidden />
        </button>
      ))}</nav>
      <div className='side-status'>
        <TbCircleDot aria-hidden size={19} />
        <div><span>활성 프로필</span><strong>{view?.phase === 'ready' ? activeName(view) : '—'}</strong></div>
      </div>
    </aside>
  )
}
