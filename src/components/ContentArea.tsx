import { useLocale } from '@/locale'
import type { ReactElement } from 'react'
import {
  TbArchive, TbArrowRight, TbArrowsExchange, TbBrandSteam, TbFolderOpen,
  TbFolders, TbPlayerPlay, TbPlugConnected, TbShield, TbShieldCheck,
} from 'react-icons/tb'
import type { Report, View } from '@/native'
import { activeName, activeProfile, canLaunchPackage, formatBytes } from '@/native'
import type { Page } from './Sidebar'
import Integrity from './Integrity'

interface Props {
  view: View | null
  report: Report | null
  desktop: boolean
  busy: boolean
  select: (page: Page) => void
  setup: () => void
  launch: () => void
  loaderInstalled: boolean
  modLaunch: () => void
  loaderSetup: () => void
  vanilla: () => void
  folder: () => void
  verify: () => void
}

export default function ContentArea({ view, report, desktop, busy, select, setup, launch, loaderInstalled, modLaunch, loaderSetup, vanilla, folder, verify }: Props): ReactElement {
  const { lang, t } = useLocale()
  const ready = view?.phase === 'ready'
  const isVanilla = ready && view.activeId === 'vanilla'
  const selected = activeProfile(view)
  const packed = selected?.kind === 'package'
  const heading = ready ? activeName(view, lang) : view?.phase === 'recovery' ? t('전환 복구 필요', 'Profile switch needs recovery') :
    view?.phase === 'awaitingVanilla' ? t('바닐라 복구 대기', 'Waiting for vanilla files') : view ? t('원본 등록 대기', 'Baseline not registered') : t('게임 폴더 연결', 'Connect Your Game Folder')
  const packageNote = view?.steamBuild ? t(`me3로 실행하면 이 프로필의 모드가 적용됩니다.${typeof selected?.bytes === 'number' ? ' 보관 용량 ' + formatBytes(selected.bytes) + '.' : ''}`, `Launch with me3 to load this profile’s mods.${typeof selected?.bytes === 'number' ? ' Storage used: ' + formatBytes(selected.bytes) + '.' : ''}`) :
    t('me3 실행에는 Steam 라이브러리에 등록된 엘든 링이 필요합니다.', 'Launching with me3 requires Elden Ring in a Steam library.')
  const note = ready ? isVanilla ? t('Steam 원본 파일을 검사한 뒤 공식 런처로 실행합니다.', 'Verify the original Steam files, then launch the game through Steam.') : packed ? packageNote : t('이전 버전의 전체 설치본을 사용 중입니다. 모드 프로필에서 경량 사본을 만들 수 있습니다.', 'This profile uses a legacy full installation. You can make a lightweight copy in Mod Profiles.') :
    view?.phase === 'awaitingVanilla' ? t('Steam에서 파일 검사를 완료한 후 바닐라 원본을 등록하세요.', 'Finish verifying your game files in Steam, then register the vanilla baseline.') :
      view?.phase === 'recovery' ? t('설치 설정에서 중단된 작업을 복구하세요.', 'Open Settings to recover the interrupted operation.') :
        view ? t('현재 설치에 모드가 있는지에 따라 등록 방법을 선택하세요.', 'Choose the setup option that matches your installation.') : t('Steam 설치 위치를 찾아 프로필 관리를 시작하세요.', 'Locate your Steam installation to start managing mod profiles.')
  const HeroIcon = ready ? isVanilla ? TbShieldCheck : TbFolders : TbPlugConnected
  return (
    <>
      <div className='page-title'><div><h1>{t('사용 현황', 'Overview')}</h1><p>{t('현재 프로필로 실행하거나 바닐라 파일을 검사하세요.', 'Launch the current profile or verify your vanilla game files.')}</p></div></div>
      <section className='hero'>
        <div className='hero-body'><div className='hero-label'><HeroIcon size={17} />{ready ? t('현재 활성 프로필', 'Active profile') : t('설치 상태', 'Installation status')}</div>
          <h2>{heading}</h2><p>{note}</p>
          <div className='actions'>{ready ? <>
            {isVanilla ? <button className='action primary' disabled={busy || !desktop || Boolean(view.stale || view.steamIssue || !view.steamBuild)}
              onClick={launch}><TbPlayerPlay />{t('바닐라 검사 후 실행', 'Verify and Launch Vanilla')}</button> : <>
              {packed && (loaderInstalled ? <button className='action primary' disabled={busy || !desktop || !canLaunchPackage(view)} onClick={modLaunch}><TbPlayerPlay />{t('모드 실행', 'Launch Mods')}</button> :
                <button className='action primary' disabled={busy || !desktop} onClick={loaderSetup}><TbPlugConnected />{t('me3 연결', 'Connect me3')}</button>)}
              <button className={packed ? 'action hero-secondary' : 'action primary'} disabled={busy || !desktop} onClick={vanilla}><TbShield />{t('바닐라로 전환', 'Switch to Vanilla')}</button></>}
            <button className='action hero-secondary' onClick={folder} disabled={busy || !desktop}><TbFolderOpen />{t('게임 폴더 열기', 'Open Game Folder')}</button>
          </> : <button className='action primary' onClick={setup} disabled={busy}>
            {view?.phase === 'recovery' ? t('복구 설정', 'Recovery Settings') : view?.phase === 'awaitingVanilla' ? t('복구 및 등록', 'Recover and Register') : t('설치 설정 열기', 'Open Settings')}<TbArrowRight /></button>}</div>
        </div>
        <div className='hero-emblem' aria-hidden><div className='emblem-ring'><TbShield /></div></div>
      </section>
      {view?.stale && <p className='callout'>{t('현재 Steam 빌드와 기준본이 다릅니다. 바닐라로 전환한 뒤 새 원본을 등록하세요.', 'The Steam build differs from the baseline. Switch to vanilla and register a new baseline.')}</p>}
      <div className='shortcuts'>
        <button className='shortcut' onClick={() => select('profiles')}><TbArrowsExchange /><span><strong>{t('모드 프로필', 'Mod Profiles')}</strong>
          <small>{view ? t(`보관 중인 모드 조합 ${view.profiles.length}개`, `${view.profiles.length} saved mod ${view.profiles.length === 1 ? 'profile' : 'profiles'}`) : t('프로필 생성 및 전환', 'Create or switch profiles')}</small></span><TbArrowRight /></button>
        <button className='shortcut' onClick={() => select('saves')}><TbArchive /><span><strong>{t('세이브 백업', 'Save Backups')}</strong>
          <small>{t('진행 상황 보관 및 복원', 'Back up and restore progress')}</small></span><TbArrowRight /></button>
      </div>
      <Integrity report={report} enabled={Boolean(ready && desktop && !busy)} verify={verify} />
      <p className='footnote'><TbBrandSteam />{t('공식 실행은 Steam을 사용합니다. 검사 범위는 Game 폴더이며 EAC의 접속 판정은 게임에서 이루어집니다.', 'The official launch uses Steam. Verification covers the Game folder. The game handles EAC’s online access checks.')}</p>
    </>
  )
}
