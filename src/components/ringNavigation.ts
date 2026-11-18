import type { IconType } from 'react-icons'
import { TbArchive, TbCalculator, TbFile, TbFolders, TbLayoutDashboard, TbNotebook, TbSettings } from 'react-icons/tb'
import { titles } from './NativeSidebar'
import type { Page } from './NativeSidebar'
import type { Language } from '@/locale'

export { titles }
export type { Page }

const englishTitles: Record<Page, string> = {
  overview: 'Overview', profiles: 'Mod Profiles', saves: 'Save Manager',
  journal: 'Adventure Journal', planner: 'Build Planner', settings: 'Settings',
}

export function pageTitle(page: Page, lang: Language): string {
  return lang === 'en' ? englishTitles[page] : titles[page]
}

interface SearchInfo {
  icon: IconType
  detail: string
  enDetail: string
  terms: string
}

const searchInfo: Partial<Record<Page, SearchInfo>> = {
  overview: { icon: TbLayoutDashboard, detail: '활성 프로필과 바닐라 무결성', enDetail: 'Active profile and vanilla file integrity', terms: '홈 대시보드 설치 상태 home dashboard status' },
  profiles: { icon: TbFolders, detail: '프로필 보관과 전환', enDetail: 'Manage and switch mod profiles', terms: '모드 보관함 생성 이름 변경 적용 경량 복제 가져오기 충돌 import duplicate conflicts' },
  saves: { icon: TbArchive, detail: '캐릭터 확인과 백업 복원', enDetail: 'View characters and restore backups', terms: '슬롯 파일 내보내기 가져오기 정기 주기 자동 보관 고정 slots export import scheduled backups' },
  journal: { icon: TbNotebook, detail: '회차별 진행 기록', enDetail: 'Track progress across journeys', terms: 'NG+ NG플러스 다회차 지도 체크리스트 메모 map checklist notes' },
  planner: { icon: TbCalculator, detail: '태생 비교와 레벨별 룬', enDetail: 'Compare starting classes and rune costs', terms: '목표 능력치 스탯 클래스 육성 레벨업 attributes stats leveling' },
  settings: { icon: TbSettings, detail: '게임 경로와 모드 로더 연결', enDetail: 'Game installation, language and appearance', terms: 'Steam 스팀 원본 등록 복구 setup restore' },
}

// The base app's route list determines which pages the themed shell exposes.
export const navigation = (Object.keys(titles) as Page[]).map(page => ({
  page,
  ...(searchInfo[page] ?? { icon: TbFile, detail: '메뉴 열기', enDetail: 'Open page', terms: '' }),
}))
