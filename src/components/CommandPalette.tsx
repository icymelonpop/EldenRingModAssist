import { useEffect, useId, useRef, useState } from 'react'
import type { KeyboardEvent, ReactElement } from 'react'
import type { IconType } from 'react-icons'
import {
  TbArrowUpRight, TbCalculator, TbCoins, TbDeviceFloppy, TbFileSearch, TbFingerprint,
  TbFolder, TbFolders, TbHistory, TbMap, TbNotebook, TbPin, TbSearch, TbSettings, TbX,
} from 'react-icons/tb'
import type { View } from '@/native'
import { useLocale } from '@/locale'
import type { Language } from '@/locale'
import { categories, categoryLabel } from './trackerCatalog'
import { navigation, pageTitle } from './ringNavigation'
import type { Page } from './ringNavigation'

export interface Destination {
  page: Page
  heading?: string
  control?: string
  selector?: string
  selectTab?: boolean
  profileId?: string
}

interface Result extends Destination {
  id: string
  label: string
  detail: string
  terms: string
  icon: IconType
}

type Copy = readonly [korean: string, english: string]
interface Section {
  id: string
  page: Page
  label: Copy
  terms: string
  icon: IconType
  detail?: Copy
  selector?: string
  focus?: 'heading' | 'control'
  target?: Copy
}

const sections: Section[] = [
  { id: 'create-profile', page: 'profiles', icon: TbFolders, focus: 'heading',
    label: ['프로필 만들기', 'Create a Profile'], terms: '경량 DLL 모드 추가 빈 프로필 복제 별도 설치본 가져오기 new empty duplicate import DLL' },
  { id: 'verify', page: 'overview', icon: TbFingerprint, focus: 'heading',
    label: ['바닐라 무결성 검사', 'Verify Vanilla Files'], terms: '원본 해시 비교 추가 변경 누락 기준 파일 전체 검사 integrity hash compare missing changed' },
  { id: 'inventory', page: 'saves', icon: TbDeviceFloppy, focus: 'heading',
    label: ['현재 세이브', 'Current Saves'], terms: 'Steam 계정 파일 캐릭터 슬롯 레벨 플레이 시간 Steam character slots level playtime' },
  { id: 'periodic', page: 'saves', icon: TbHistory, focus: 'heading',
    label: ['주기 자동 백업', 'Scheduled Backups'], terms: '정기 백업 자동 저장 시간 간격 보관 개수 정리 automatic periodic backup retention interval' },
  { id: 'backups', page: 'saves', icon: TbHistory, focus: 'heading',
    label: ['백업 목록', 'Backups'], terms: '세이브 복원 복사 가져오기 내보내기 restore copy import export' },
  { id: 'pin-backups', page: 'saves', icon: TbPin, focus: 'heading', target: ['백업 목록', 'Backups'],
    label: ['자동 백업 보관 고정', 'Pin Automatic Backups'], detail: ['백업 목록 · 주기 백업', 'Backups · Scheduled backups'], terms: '정기 백업 계속 보관 고정 해제 보호 정리 retention keep protected' },
  { id: 'mod-review', page: 'profiles', icon: TbFileSearch, focus: 'control', target: ['모드 폴더로 프로필 만들기', 'Create Profile from Mod Folder'],
    label: ['모드 파일 충돌 미리보기', 'Preview Mod Conflicts'], detail: ['모드 폴더로 프로필 만들기', 'Create Profile from Mod Folder'], terms: '덮어쓰기 중복 추가 변경 유지 regulation.bin duplicate overwrite keep regulation.bin' },
  { id: 'journey', page: 'journal', icon: TbNotebook, selector: '#journal-new',
    label: ['회차별 여정 관리', 'Manage Journeys'], terms: 'NG+ 새 게임 플러스 캐릭터 다회차 새 여정 기록 new game plus NG+ playthrough' },
  { id: 'journal-map', page: 'journal', icon: TbMap, selector: '#journal-map',
    label: ['한글 지도', 'Korean Map'], terms: '지역 위치 탐색 맵 locations interactive map' },
  { id: 'origins', page: 'planner', icon: TbCalculator, selector: '#target-attributes',
    label: ['태생별 능력치 비교', 'Compare Starting Classes'], terms: '클래스 목표 스탯 직업 시작 레벨 최저 레벨 origins starting class target attributes stats' },
  { id: 'runes', page: 'planner', icon: TbCoins, selector: '#rune-calculator',
    label: ['레벨 구간 룬 계산', 'Rune Calculator'], terms: '룬 계산기 레벨업 비용 목표 레벨 level range cost level up' },
  { id: 'path', page: 'settings', icon: TbFolder, focus: 'heading',
    label: ['게임 설치 위치', 'Game Installation'], terms: 'Game Steam 스팀 경로 폴더 선택 연결 감지 Steam find browse game folder location' },
  { id: 'language', page: 'settings', icon: TbSettings, selector: '#interface-language',
    label: ['표시 언어', 'Display Language'], detail: ['설치 설정 · 화면 효과', 'Settings · Appearance'], terms: '한국어 영어 언어 설정 language languages Korean English locale' },
  { id: 'appearance', page: 'settings', icon: TbSettings, selector: '.ring-mode-picker input:checked',
    label: ['화면 모드', 'Color Mode'], detail: ['설치 설정 · 화면 효과', 'Settings · Appearance'], terms: '화이트 블랙 라이트 다크 모드 밝은 어두운 테마 색상 light dark theme appearance' },
  { id: 'glass', page: 'settings', icon: TbSettings, selector: '#background-glass',
    label: ['배경 비침', 'Transparency'], detail: ['설치 설정 · 화면 효과', 'Settings · Appearance'], terms: '반투명 투명도 불투명 흐림 효과 유리 창 배경 테마 translucent glass opaque blur backdrop' },
  { id: 'loader', page: 'settings', icon: TbSettings, focus: 'heading',
    label: ['me3 모드 로더', 'me3 Mod Loader'], terms: '경량 프로필 실행 로더 다운로드 자동 찾기 경로 path download locate lightweight loader' },
]

function sectionResults(lang: Language): Result[] {
  const local = ([ko, en]: Copy): string => lang === 'en' ? en : ko
  return [...sections.map(section => ({
    id: section.id, page: section.page, icon: section.icon, selector: section.selector,
    label: local(section.label), detail: section.detail ? local(section.detail) : pageTitle(section.page, lang),
    terms: [...section.label, section.terms].join(' '),
    heading: section.focus === 'heading' ? local(section.target ?? section.label) : undefined,
    control: section.focus === 'control' ? local(section.target ?? section.label) : undefined,
  })), ...categories.map(category => ({
    id: 'journal-' + category.id, page: 'journal' as const, selector: 'button[data-journal-tab="' + category.id + '"]', selectTab: true,
    label: categoryLabel(category.id, lang), detail: pageTitle('journal', lang),
    terms: [category.name, categoryLabel(category.id, 'en'), '회차 수집 진행 체크리스트 journey collectibles checklist'].join(' '), icon: TbNotebook,
  }))]
}

function normalize(text: string): string {
  return text.normalize('NFKC').toLocaleLowerCase('ko').replace(/\s+/g, '')
}

function matches(view: View | null, query: string, lang: Language): Result[] {
  const pages = navigation.map(({ page, icon, terms, detail, enDetail }) => ({
    id: page, page, icon, terms: [terms, pageTitle(page, 'en'), pageTitle(page, 'ko')].join(' '),
    label: pageTitle(page, lang), detail: lang === 'en' ? enDetail : detail,
  }))
  const words = query.trim().split(/\s+/).map(normalize).filter(Boolean)
  if (!words.length) return pages
  const profiles = [...(view?.phase === 'ready' ? [{ id: 'vanilla', name: lang === 'en' ? 'Vanilla' : '바닐라' }] : []), ...(view?.profiles ?? [])]
  const all: Result[] = [...pages, ...sectionResults(lang), ...profiles.map(profile => ({
    id: 'profile-' + profile.id, page: 'profiles' as const, profileId: profile.id, label: profile.name,
    detail: lang === 'en' ? 'Profile Library' : '프로필 보관함', terms: '모드 프로필 mod profile', icon: TbFolders,
  }))]
  // Prefer the exact name, then matching words in the label over related keywords.
  const phrase = words.join('')
  const rank = (item: Result): number => {
    const label = normalize(item.label)
    return label === phrase ? words.length + 1 : words.filter(word => label.includes(word)).length
  }
  return all.filter(item => {
    const text = normalize([item.label, item.detail, item.terms].join(' '))
    return words.every(word => text.includes(word))
  }).sort((a, b) => rank(b) - rank(a))
}

interface Props {
  view: View | null
  go: (destination: Destination) => void
  close: () => void
}

export default function CommandPalette({ view, go, close }: Props): ReactElement {
  const dialog = useRef<HTMLDialogElement>(null)
  const input = useRef<HTMLInputElement>(null)
  const [query, setQuery] = useState('')
  const [active, setActive] = useState(0)
  const { lang, t } = useLocale()
  const listId = useId()
  const results = matches(view, query, lang)
  const selected = Math.max(0, Math.min(active, results.length - 1))
  const selectedId = results.length ? listId + '-' + selected : undefined

  useEffect(() => {
    const element = dialog.current
    element?.showModal()
    input.current?.focus()
    return () => element?.close()
  }, [])

  useEffect(() => {
    if (selectedId) document.getElementById(selectedId)?.scrollIntoView({ block: 'nearest' })
  }, [selectedId])

  function activate(result: Result): void {
    dialog.current?.close()
    go(result)
  }

  function dismiss(): void {
    // Closing while mounted lets the browser restore focus to the invoking control.
    dialog.current?.close()
    close()
  }

  function keydown(event: KeyboardEvent<HTMLInputElement>): void {
    if (event.nativeEvent.isComposing || event.nativeEvent.keyCode === 229 || !results.length) return
    if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
      event.preventDefault()
      const direction = event.key === 'ArrowDown' ? 1 : -1
      setActive((selected + direction + results.length) % results.length)
    } else if (event.key === 'Enter') {
      event.preventDefault()
      activate(results[selected])
    }
  }

  return (
    <dialog ref={dialog} className='dialog ring-palette' aria-label={t('메뉴 및 프로필 검색', 'Search menus and profiles')}
      onCancel={event => { event.preventDefault(); dismiss() }}
      onClick={event => {
        if (event.target !== event.currentTarget) return
        const bounds = event.currentTarget.getBoundingClientRect()
        if (event.clientX < bounds.left || event.clientX > bounds.right || event.clientY < bounds.top || event.clientY > bounds.bottom) dismiss()
      }}>
      <div className='ring-palette-search'>
        <TbSearch size={20} aria-hidden />
        <input ref={input} role='combobox' aria-label={t('검색어', 'Search query')} aria-autocomplete='list' aria-expanded
          aria-controls={listId} aria-activedescendant={selectedId} autoComplete='off' spellCheck={false}
          value={query} placeholder={t('메뉴, 기능 또는 프로필 이름 검색', 'Search menus, features or profiles')} onKeyDown={keydown}
          onChange={event => { setQuery(event.target.value); setActive(0) }} />
        <button type='button' className='icon-btn' onClick={dismiss} aria-label={t('검색 닫기', 'Close search')} title={t('검색 닫기', 'Close search')}><TbX size={18} aria-hidden /></button>
      </div>
      <div className='ring-result-list' id={listId} role='listbox' aria-label={t('검색 결과', 'Search results')}>
        {results.map((result, index) => (
          <button type='button' key={result.id} id={listId + '-' + index} className='ring-result'
            role='option' aria-selected={index === selected} tabIndex={-1}
            onMouseDown={event => event.preventDefault()} onClick={() => activate(result)}>
            <result.icon size={19} aria-hidden /><span><strong>{result.label}</strong><small>{result.detail}</small></span><TbArrowUpRight size={16} aria-hidden />
          </button>
        ))}
      </div>
      {!results.length && <p className='ring-no-results' role='status'>{t('일치하는 메뉴나 프로필이 없습니다.', 'No matching menus or profiles.')}</p>}
    </dialog>
  )
}
