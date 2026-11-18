import { translateNative } from '@/nativeMessages'
import { useLocale } from '@/locale'
import type { ReactElement } from 'react'
import { useEffect, useRef, useState } from 'react'
import { TbAlertTriangle, TbCheck, TbDeviceDesktop, TbLoader2, TbX } from 'react-icons/tb'
import ContentArea from './components/ContentArea'
import { ConfirmDialog } from './components/Controls'
import type { Question } from './components/Controls'
import Header from './components/Header'
import Integrity from './components/Integrity'
import ModLoader from './components/ModLoader'
import ModConflictDialog from './components/ModConflictDialog'
import type { ModPreview } from './components/ModConflictDialog'
import Profiles from './components/Profiles'
import Saves, { backupDate, fileKey } from './components/Saves'
import type { Peek } from './components/Saves'
import Settings from './components/Settings'
import ProgressTracker from './components/ProgressTracker'
import BuildPlanner from './components/BuildPlanner'
import Sidebar from './components/Sidebar'
import type { Page } from './components/Sidebar'
import { activeName, activeProfile, formatBytes, hasDesktop, native } from './native'
import type { Backup, LoaderStatus, Report, SaveFile, Slot, View } from './native'

interface LocalizedMessage { ko: string; en: string }
interface Feedback { message: string | LocalizedMessage; error: boolean }

function message(ko: string, en: string): LocalizedMessage {
  return { ko, en }
}

function failureText(value: unknown): string | LocalizedMessage {
  if (value && typeof value === 'object' && 'ko' in value && 'en' in value &&
      typeof value.ko === 'string' && typeof value.en === 'string') return { ko: value.ko, en: value.en }
  return value instanceof Error ? value.message : String(value)
}
interface Progress { done: number; total: number }
const PATH_KEY = 'elden-ring-mod-assist.game-path'
const BACKUP_KEY = 'elden-ring-mod-assist.backup-before-switch'
function lastPath(): string {
  try { return localStorage.getItem(PATH_KEY) ?? '' } catch { return '' }
}
function savePath(path: string): void {
  try { localStorage.setItem(PATH_KEY, path) } catch { /* Session remains usable without storage. */ }
}

export default function App(): ReactElement {
  const { lang, t } = useLocale()
  const [page, setPage] = useState<Page>('overview')
  const [desktop, setDesktop] = useState<boolean | null>(null)
  const [view, setView] = useState<View | null>(null)
  const [loader, setLoader] = useState<LoaderStatus | null>(null)
  const [draft, setDraft] = useState('')
  const [report, setReport] = useState<Report | null>(null)
  const [auditReport, setAuditReport] = useState<Report | null>(null)
  const [backups, setBackups] = useState<Backup[] | null>(null)
  const [listedBackups, setListedBackups] = useState(false)
  const [inventory, setInventory] = useState<SaveFile[] | null>(null)
  const [autoBackup, setAutoBackup] = useState(true)
  const [peek, setPeek] = useState<Peek | null>(null)
  const [busy, setBusy] = useState<LocalizedMessage | null>(null)
  const [progress, setProgress] = useState<Progress | null>(null)
  const [feedback, setFeedback] = useState<Feedback | null>(null)
  const [question, setQuestion] = useState<Question | null>(null)
  const [modReview, setModReview] = useState<{ source: string; review: ModPreview } | null>(null)
  const opLock = useRef(false)
  const desktopReady = desktop === true
  const disabled = !desktopReady || !!busy
  const gamePath = view?.gamePath
  const main = useRef<HTMLElement>(null)

  function acceptView(next: View): void {
    setView(next)
    setDraft(next.gamePath)
    savePath(next.gamePath)
    setReport(null)
    setAuditReport(null)
  }

  useEffect(() => {
    let alive = true
    const available = hasDesktop()
    setDesktop(available)
    if (!available) return
    try { setAutoBackup(localStorage.getItem(BACKUP_KEY) !== 'false') } catch { /* default to enabled */ }
    opLock.current = true
    setBusy(message('설치 상태 확인 중', 'Checking installation'))
    async function init(): Promise<void> {
      let path = lastPath()
      try {
        path ||= (await native<string | null>('detect_game')) ?? ''
        if (alive) setDraft(path)
        if (!path) return
        const initial = await native<View>('status', { gamePath: path })
        if (alive) acceptView(initial)
      } catch (error) {
        if (alive) setFeedback({ error: true, message: failureText(error) })
      } finally {
        if (alive) { opLock.current = false; setBusy(null) }
      }
    }
    void init()
    void native<LoaderStatus>('loader_status').then(status => {
      if (alive) setLoader(status)
    }).catch(() => { /* Detection can be retried in settings. */ })
    return () => { alive = false }
  }, [])

  useEffect(() => {
    if (!busy || !desktopReady) return
    let alive = true
    let pending = false
    const poll = window.setInterval(async () => {
      if (pending) return
      pending = true
      try {
        const current = await native<Progress>('progress')
        if (alive) setProgress(current.total > 0 ? current : null)
      } catch { /* The operation itself reports any errors. */ }
      finally { pending = false }
    }, 750)
    return () => { alive = false; window.clearInterval(poll) }
  }, [busy, desktopReady])

  async function run(label: LocalizedMessage, work: () => Promise<void>, success?: LocalizedMessage): Promise<void> {
    if (!hasDesktop() || opLock.current) return
    opLock.current = true
    setProgress(null)
    setFeedback(null)
    setBusy(label)
    main.current?.scrollTo({ top: 0 })
    try {
      await work()
      if (success) setFeedback({ error: false, message: success })
    } catch (error) {
      setFeedback({ error: true, message: failureText(error) })
    } finally {
      opLock.current = false
      setBusy(null)
      setProgress(null)
    }
  }

  async function connectTo(path: string): Promise<void> {
    if (path.trim()) acceptView(await native<View>('status', { gamePath: path.trim() }))
  }

  async function findGame(): Promise<void> {
    const path = await native<string | null>('detect_game')
    if (!path) throw message('Steam 라이브러리에서 엘든 링을 찾지 못했습니다. 설치한 Game 폴더를 선택하세요.', 'Elden Ring was not found in your Steam libraries. Select the game\'s Game folder.')
    await connectTo(path)
  }

  async function loadBackups(): Promise<void> {
    setPeek(null)
    const results = await Promise.allSettled([
      native<Backup[]>('list_backups').then(setBackups),
      native<SaveFile[]>('list_save_files').then(setInventory),
    ])
    const errors = results.filter(result => result.status === 'rejected')
    if (errors.length) {
      const reasons = errors.map(err => String(err.reason instanceof Error ? err.reason.message : err.reason))
      throw message(reasons.join('\n'), reasons.map(reason => translateNative(reason, 'en')).join('\n'))
    }
  }

  useEffect(() => {
    if (page !== 'saves' || !desktopReady || busy || listedBackups) return
    setListedBackups(true)
    void run(message('백업 목록 확인 중', 'Loading backups'), loadBackups)
  }, [page, desktopReady, busy, listedBackups])

  function go(to: Page): void {
    setPage(to)
    main.current?.scrollTo({ top: 0 })
  }

  function loaderSetup(): void {
    go('settings')
    window.requestAnimationFrame(() => document.getElementById('mod-loader')?.focus())
  }

  async function detectLoader(): Promise<void> {
    setLoader(await native<LoaderStatus>('loader_status'))
  }

  function refresh(): void {
    if (page === 'saves') { void run(message('백업 목록 확인 중', 'Loading backups'), loadBackups); return }
    void run(message('설치 상태 확인 중', 'Checking installation'), async () => {
      const path = gamePath || draft.trim()
      await Promise.all([path ? connectTo(path) : findGame(), detectLoader()])
    })
  }

  function mutate(command: string, args: Record<string, string | boolean | string[]>, label: LocalizedMessage, success: LocalizedMessage): void {
    if (!gamePath) return
    void run(label, async () => {
      try {
        acceptView(await native<View>(command, { gamePath, ...args }))
        if (command === 'switch_profile') setListedBackups(false)
      }
      catch (error) {
        // A partially completed file move can leave a recoverable journal.
        try { acceptView(await native<View>('status', { gamePath })) }
        catch { setView(null); setReport(null); setAuditReport(null) }
        throw error
      }
    }, success)
  }

  function onboard(): void {
    setQuestion({
      title: t('현재 설치에서 모드를 분리할까요?', 'Separate the mods from this installation?'),
      message: t('Steam 배포본과 비교해 추가·변경된 파일만 별도 폴더에 보관합니다. 분리 후 Steam 파일 검사에서 원본을 복구하세요. 별도의 설치가 필요한 파일은 프로필 폴더의 preserved에서 확인하세요.', 'Files added to or changed from the Steam release will be stored separately. Then verify your game files in Steam to restore the originals. Files that need separate installation are kept in the profile’s preserved folder.'),
      input: { label: t('프로필 이름', 'Profile name'), initial: t('기존 모드 구성', 'Existing mod setup') }, button: t('프로필로 보관', 'Save as Profile'),
      action: name => mutate('onboard_modded', { name }, message('추가·변경 파일 보관 중', 'Archiving added and modified files'), message('모드 파일을 분리했습니다. Steam 파일 검사로 원본을 복구하세요.', 'Mod files were separated. Verify your game files in Steam to restore the originals.')),
    })
  }

  function baseline(): void {
    setQuestion({
      title: view?.phase === 'ready' ? t('바닐라 원본을 다시 등록할까요?', 'Register the vanilla baseline again?') : t('바닐라 원본을 등록할까요?', 'Register the vanilla baseline?'),
      message: view?.steamBuild ? t('Steam 공식 배포 파일과 게임 폴더 전체를 대조한 뒤 기준 파일을 기록합니다. 다운로드와 업데이트가 끝난 뒤 진행하세요.', 'The entire Game folder will be compared with Steam’s official files before recording the baseline. Finish any downloads or updates first.') :
        t('현재 폴더를 사용자가 확인한 로컬 원본으로 기록합니다. Steam 온라인 실행에는 Steam 라이브러리에서 등록한 원본이 필요합니다.', 'This folder will be recorded as a local baseline you have verified. To launch online through Steam, register the original installation from a Steam library.'),
      check: t('모드를 제거한 깨끗한 원본이며 설치가 완료되었습니다.', 'I have removed all mods and finished installing the original game.'), button: t('검사 후 등록', 'Verify and Register'),
      action: () => mutate('baseline', {}, message('바닐라 원본 확인 및 등록 중', 'Verifying and registering vanilla files'), message('바닐라 원본을 등록했습니다.', 'The vanilla baseline has been registered.')),
    })
  }

  function create(): void {
    const empty = view?.activeId === 'vanilla'
    setQuestion({
      title: empty ? t('빈 프로필 만들기', 'Create an Empty Profile') : t('현재 조합으로 새 프로필 만들기', 'Create a Profile from the Current Setup'),
      message: empty ? t('공유할 게임 원본은 그대로 두고 모드를 담을 폴더와 me3 프로필을 만듭니다.', 'Create a mod folder and an me3 profile that share the original game files.') :
        t(`「${activeName(view)}」의 모드 파일만 담은 독립 프로필을 만듭니다. 이후 각 프로필의 모드를 따로 편집할 수 있습니다.`, `Create an independent profile containing only the mods from “${activeName(view, 'en')}”. You can edit each profile separately.`),
      input: { label: t('새 프로필 이름', 'New profile name'), initial: '' }, button: t('프로필 만들기', 'Create Profile'),
      action: name => mutate('create_profile', { name }, message('경량 프로필 생성 중', 'Creating a lightweight profile'), message('프로필을 만들었습니다. 보관함에서 전환할 수 있습니다.', 'Profile created. You can switch to it in the profile library.')),
    })
  }

  function importProfile(mods: boolean): void {
    void run(mods ? message('모드 폴더 선택 및 파일 비교 중', 'Selecting a mod folder and comparing files') : message('가져올 게임 폴더 선택 중', 'Selecting a game folder to import'), async () => {
      const source = await native<string | null>('pick_source')
      if (!source) return
      if (mods) {
        const review = await native<ModPreview>('preview_mods', { gamePath, source })
        setModReview({ source, review })
        return
      }
      setQuestion({
        title: t('외부 게임 폴더 가져오기', 'Import an External Game Folder'),
        message: t(`선택한 Game 폴더와 등록한 바닐라를 비교합니다. 새 프로필에는 추가되거나 변경된 모드 파일만 보관합니다.\n\n${source}`, `Compare the selected Game folder with the vanilla baseline. The new profile will contain only added and modified mod files.\n\n${source}`),
        input: { label: t('새 프로필 이름', 'New profile name'), initial: '' }, button: t('프로필 만들기', 'Create Profile'),
        action: name => mutate('import_profile', { source, name }, message('모드 프로필 복사 중', 'Copying the mod profile'), message('새 프로필을 보관함에 추가했습니다.', 'The new profile has been added to your library.')),
      })
    })
  }

  function addNative(): void {
    void run(message('DLL 모드 폴더와 실행할 DLL 선택 중', 'Selecting a DLL mod folder and entry point'), async () => {
      const source = await native<string | null>('pick_source')
      if (!source) return
      const entry = await native<string | null>('pick_native_entry', { source })
      if (!entry) return
      setQuestion({
        title: t('DLL 모드로 프로필 만들기', 'Create a Profile from a DLL Mod'), button: t('DLL 프로필 만들기', 'Create DLL Profile'),
        message: t(`「${activeName(view)}」에 이 모드 폴더를 더합니다. 설정 파일과 종속 파일을 함께 보관하며, me3가 아래 DLL을 실행 항목으로 사용합니다.\n\n모드 폴더\n${source}\n\n실행할 DLL\n${entry}`, `Add this mod folder to “${activeName(view, 'en')}”. Configuration files and dependencies stay together. me3 will load the selected DLL.\n\nMod folder\n${source}\n\nDLL to load\n${entry}`),
        input: { label: t('새 프로필 이름', 'New profile name'), initial: '' },
        action: name => mutate('profile_from_native', { source, entry, name }, message('DLL 프로필 생성 중', 'Creating a DLL profile'), message('지정한 DLL과 모드 파일을 새 프로필에 보관했습니다.', 'The selected DLL and mod files have been saved in a new profile.')),
      })
    })
  }

  function activate(id: string, name: string): void {
    const target = view?.profiles.find(profile => profile.id === id)
    const current = activeProfile(view)
    const legacy = (target && target.kind !== 'package') || (current && current.kind !== 'package')
    setQuestion({
      title: t(`「${name}」로 전환할까요?`, `Switch to “${name}”?`), button: t('프로필 전환', 'Switch Profile'),
      message: (legacy ? t('이전 전체 설치본과 전환하기 위해 게임 폴더 위치를 바꿉니다. ', 'The game folder will be moved to switch to or from a legacy full installation. ') : '') +
        (id === 'vanilla' ? t('바닐라 원본을 선택합니다. 전환 후 사용 현황에서 검사하고 Steam으로 실행할 수 있습니다.', 'Select the vanilla baseline. You can then verify the files and launch through Steam from Overview.') :
          target?.kind === 'package' ? t('공유하는 게임 원본과 함께 사용할 모드 조합을 선택합니다. 전환 후 모드 실행을 누르세요.', 'Select the mods to use with the shared game files. After switching, choose Launch Mods.') :
            t('전체 설치본을 활성화합니다. 실행 방법은 해당 모드의 안내를 따르세요.', 'Activate the full installation. Follow the mod author’s launch instructions.')) +
        (autoBackup ? t(' 전환 직전 세이브를 자동 백업합니다.', ' Your saves will be backed up automatically before switching.') : t(' 이번 전환의 세이브 자동 백업은 꺼져 있습니다.', ' Automatic save backups are off for this switch.')),
      action: () => mutate('switch_profile', { id, saveBackup: autoBackup }, message('프로필 전환 중', 'Switching profiles'), message(`「${id === 'vanilla' ? '바닐라' : name}」로 전환했습니다.`, `Switched to “${id === 'vanilla' ? 'Vanilla' : name}”.`)),
    })
  }

  function convert(id: string, previous: string): void {
    setQuestion({
      title: t('전체 설치본에서 경량 사본을 만들까요?', 'Create a lightweight copy of the full installation?'),
      message: t(`「${previous}」를 바닐라와 비교해 변경·추가된 모드 파일을 새 프로필에 저장합니다. 기존 설치본도 계속 사용할 수 있습니다. 경량 사본을 확인한 뒤 기존 설치본은 보관 해제하고 정리할 수 있습니다.`, `Compare “${previous}” against vanilla and store its changed and added mod files in a new profile. You can keep using the full installation or remove it from the library after verifying the copy.`),
      input: { label: t('새 프로필 이름', 'New profile name'), initial: t(`${previous.slice(0, 57)} (경량)`, `${previous.slice(0, 50)} (lightweight)`) }, button: t('경량 사본 만들기', 'Create Lightweight Copy'),
      action: name => mutate('convert_profile', { id, name }, message('기존 설치본 비교 중', 'Comparing the existing installation'), message('경량 프로필을 추가했습니다. 전환 후 모드를 확인하세요.', 'Lightweight profile added. Switch to it to check the mods.')),
    })
  }

  function launchModded(): void {
    void run(message('me3로 모드 실행 중', 'Launching mods with me3'), async () => {
      try { await native('launch_modded', { gamePath }) }
      catch (error) { await detectLoader().catch(() => {}); throw error }
    }, message('me3에 선택한 프로필의 실행을 요청했습니다.', 'The launch request was sent to me3 for the selected profile.'))
  }

  function steam(): void {
    setQuestion({
      title: t('Steam 파일 검사를 열까요?', 'Open Steam file verification?'), button: t('Steam 열기', 'Open Steam'),
      message: t('Steam이 공식 파일을 확인하고 필요한 파일을 다시 다운로드합니다. 진행 상황은 Steam의 다운로드 화면에서 확인하세요.', 'Steam will verify the official files and download any missing or changed originals. Follow the progress on Steam’s Downloads page.'),
      action: () => { void run(message('Steam 검사 여는 중', 'Opening Steam file verification'), () => native('steam_verify', { gamePath }), message('Steam에 파일 검사를 요청했습니다. 완료 후 설치 상태를 새로고침하세요.', 'File verification requested in Steam. Refresh the installation status when it finishes.')) },
    })
  }

  function verify(): void {
    void run(message('바닐라 전체 파일 검사 중', 'Verifying all vanilla files'), async () => {
      setReport(null)
      setReport(await native<Report>('verify_vanilla', { gamePath }))
    })
  }

  function audit(): void {
    void run(message('Steam 원본과 전체 파일 비교 중', 'Comparing files with the Steam release'), async () => {
      setAuditReport(null)
      setAuditReport(await native<Report>('audit_install', { gamePath }))
    })
  }

  function folder(): void {
    void run(message('게임 폴더 여는 중', 'Opening the game folder'), () => native('open_game_folder', { gamePath }))
  }

  return (
    <div className='app-shell'>
      <Sidebar page={page} select={go} view={view} />
      <div className='workspace'>
        <Header page={page} refresh={refresh} disabled={disabled} />
        <main ref={main} id='workspace' className='main'><div className='content'>
          {desktop === false && page !== 'journal' && page !== 'planner' && <div className='notice'><TbDeviceDesktop /><span>{t('파일 관리 기능은 Windows 데스크톱 앱에서 이용할 수 있습니다.', 'File management is available in the Windows desktop app.')}</span></div>}
          {busy && <div className='notice busy' role='status'><TbLoader2 className='spinner' /><div className='progress-body'>
            <span>{busy[lang]}… {progress && ` ${Math.min(100, Math.floor(100 * progress.done / progress.total))}%`}</span>
            {progress && <><progress aria-label={busy[lang]} value={progress.done} max={progress.total} /><small>{formatBytes(progress.done)} / {formatBytes(progress.total)}</small></>}
          </div></div>}
          {feedback && <div className={`notice ${feedback.error ? 'warning' : 'success'}`} role={feedback.error ? 'alert' : 'status'}>
            {feedback.error ? <TbAlertTriangle /> : <TbCheck />}<span>{typeof feedback.message === 'string' ? translateNative(feedback.message, lang) : feedback.message[lang]}</span>
            <button className='icon-btn' aria-label={t('알림 닫기', 'Dismiss notification')} onClick={() => setFeedback(null)}><TbX size={16} /></button></div>}
          {view?.steamIssue && <p className='callout' role='status'>{translateNative(view.steamIssue, lang)}</p>}
          {page === 'overview' && <ContentArea view={view} report={report} busy={!!busy} desktop={desktopReady} select={go}
            setup={() => go('settings')} vanilla={() => activate('vanilla', t('바닐라', 'Vanilla'))} folder={folder} verify={verify}
            loaderInstalled={Boolean(loader?.installed)} modLaunch={launchModded} loaderSetup={loaderSetup}
            launch={() => setQuestion({ title: t('바닐라 검사 후 실행할까요?', 'Verify vanilla files and launch?'), button: t('검사 후 Steam 실행', 'Verify and Launch Steam'),
              message: t('바닐라 원본과 현재 설치를 검사하고, 일치하면 Steam에서 엘든 링을 실행합니다. 온라인에 사용할 세이브를 먼저 확인하세요.', 'The app will compare the installation with the vanilla baseline and launch Elden Ring through Steam if the files match. Check which save you plan to use online first.'),
              action: () => { void run(message('실행 전 바닐라 검사 중', 'Verifying vanilla files before launch'), () => native('launch_vanilla', { gamePath }), message('Steam에 바닐라 실행을 요청했습니다.', 'Steam was asked to launch the vanilla game.')) },
            })} />}
          {page === 'profiles' && <Profiles disabled={disabled} view={view} create={create} importProfile={() => importProfile(false)} addMods={() => importProfile(true)} addNative={addNative} activate={activate}
            loaderInstalled={Boolean(loader?.installed)} launch={launchModded} loaderSetup={loaderSetup} convert={convert}
            autoBackup={autoBackup} setAutoBackup={value => { setAutoBackup(value); try { localStorage.setItem(BACKUP_KEY, String(value)) } catch { /* session setting */ } }}
            rename={(id, name) => setQuestion({ title: t('프로필 이름 변경', 'Rename Profile'), message: t('보관함에 표시할 이름을 입력하세요.', 'Enter the name to display in your profile library.'),
              input: { label: t('프로필 이름', 'Profile name'), initial: name }, button: t('이름 저장', 'Save Name'),
              action: newName => mutate('rename_profile', { id, name: newName }, message('이름 저장 중', 'Saving profile name'), message('프로필 이름을 변경했습니다.', 'Profile renamed.')),
            })} release={(id, name) => setQuestion({ title: t(`「${name}」 보관을 해제할까요?`, `Remove “${name}” from the library?`), button: t('보관 해제', 'Remove from Library'),
              message: t('모드 파일 전체를 보관함 밖의 별도 폴더로 옮기고 프로필 목록에서 제외합니다. 이동한 경로는 작업 후 표시됩니다.', 'All mod files will be moved to a separate folder outside the library, and the profile will be removed from the list. The destination will be shown afterward.'),
              action: () => { void run(message('프로필 보관 해제 중', 'Moving the profile out of the library'), async () => {
                const destination = await native<string>('release_profile', { gamePath, id })
                if (gamePath) await connectTo(gamePath)
                setFeedback({ error: false, message: message(`보관을 해제했습니다. 파일이 있는 폴더\n${destination}`, `Profile removed from the library. Files are in:\n${destination}`) })
              }) },
            })} folder={id => { void run(message('프로필 폴더 여는 중', 'Opening the profile folder'), () => native('open_profile_folder', { gamePath, id })) }} settings={() => go('settings')} />}
          {page === 'journal' && <ProgressTracker openMap={() => {
            if (!hasDesktop()) { window.open('https://edring.lsdl.kr/map', '_blank', 'noopener,noreferrer'); return }
            void native<void>('open_korean_map').catch(err => setFeedback({ message: failureText(err), error: true }))
          }} />}
          {page === 'planner' && <BuildPlanner />}
          {page === 'saves' && <Saves backups={backups} inventory={inventory} peek={peek} disabled={disabled}
            onBackupsChanged={async () => setBackups(await native<Backup[]>('list_backups'))} load={() => { void run(message('세이브 목록 확인 중', 'Loading save files'), loadBackups) }}
            viewSlots={file => { void run(message('캐릭터 슬롯 확인 중', 'Reading character slots'), async () => {
              setPeek(null)
              setPeek({ key: fileKey(file), slots: await native<Slot[]>('save_slots', { account: file.account, name: file.name }) })
            }) }}
            make={(label, account, format) => { void run(message('세이브 백업 중', 'Backing up saves'), async () => {
              setBackups(await native<Backup[]>('backup_selected', { label, account, format }))
            }, message('선택한 세이브를 백업했습니다.', 'Selected save files backed up.')) }}
            importBackup={() => { void run(message('가져올 백업 폴더 선택 중', 'Selecting a backup folder to import'), async () => {
              const imported = await native<Backup[] | null>('import_backup')
              if (imported) { setBackups(imported); setFeedback({ error: false, message: message('백업을 가져왔습니다.', 'Backup imported.') }) }
            }) }}
            exportBackup={backup => { void run(message('백업을 내보낼 위치 선택 중', 'Selecting a backup export location'), async () => {
              const destination = await native<string | null>('export_backup', { id: backup.id })
              if (destination) setFeedback({ error: false, message: message(`백업을 내보냈습니다.\n${destination}`, `Backup exported.\n${destination}`) })
            }) }}
            restore={backup => setQuestion({ title: t('선택한 세이브를 복원할까요?', 'Restore the selected save?'), button: t('백업 후 복원', 'Back Up and Restore'),
              message: t(`${backupDate(backup.created)}에 보관한 ${backup.files}개 파일을 해당 계정에 복원합니다. 현재 세이브를 먼저 자동 백업한 뒤 복원을 진행합니다.`, `Restore ${backup.files === 1 ? 'one file' : `${backup.files} files`} from ${backupDate(backup.created, 'en')} to the corresponding account. Your current saves will be backed up first.`),
              action: () => { void run(message('현재 세이브 백업 및 복원 중', 'Backing up current saves and restoring'), async () => {
                setBackups(await native<Backup[]>('restore_saves', { id: backup.id }))
                setListedBackups(false)
              }, message('선택한 세이브를 복원했습니다.', 'Selected save restored.')) },
            })} />}
          {page === 'settings' && <>
            <Settings view={view} disabled={disabled} path={draft} setPath={value => {
              setDraft(value)
              if (value !== gamePath) { setView(null); setReport(null); setAuditReport(null) }
            }} connect={() => { void run(message('폴더 연결 중', 'Connecting the game folder'), () => connectTo(draft)) }}
              browse={() => { void run(message('게임 폴더 선택 중', 'Selecting the game folder'), async () => {
                const path = await native<string | null>('pick_game')
                if (path) await connectTo(path)
              }) }} detect={() => { void run(message('Steam에서 설치 찾는 중', 'Searching Steam libraries'), findGame) }}
              onboard={onboard} baseline={baseline} steam={steam} folder={folder}
              undo={id => setQuestion({ title: t('기존 설치를 되돌릴까요?', 'Restore the original installation?'), button: t('보관 취소', 'Undo Archive'),
                message: view?.profiles.find(profile => profile.id === id)?.kind === 'package' ?
                  t('분리한 모드 파일을 원래 위치로 되돌립니다. Steam 복구가 시작되기 전에 진행하세요.', 'Return the separated mod files to their original location. Do this before Steam starts its repair.') :
                  t('보관한 전체 설치본을 원래 Game 위치로 돌려놓습니다. Steam이 Game 폴더를 다시 만들기 전이어야 합니다.', 'Move the archived full installation back to its original Game folder. Do this before Steam recreates the folder.'),
                action: () => mutate('undo_park', { id }, message('기존 설치 되돌리는 중', 'Restoring the previous installation'), message('기존 설치를 되돌렸습니다.', 'Original installation restored.')),
              })}
              recover={() => setQuestion({ title: t('중단된 전환을 복구할까요?', 'Recover the interrupted switch?'), button: t('복구 시작', 'Start Recovery'),
                message: t('기록된 작업과 실제 폴더 위치를 확인하고 전환을 마무리합니다.', 'Check the operation log and current folder locations, then finish switching profiles.'),
                action: () => mutate('recover', {}, message('전환 복구 중', 'Recovering the profile switch'), message('전환 작업을 복구했습니다.', 'Profile switch recovered.')),
              })} />
            {view && view.phase !== 'recovery' && (view.phase !== 'ready' || view.activeId === 'vanilla' || activeProfile(view)?.kind === 'package') &&
              <Integrity source='steam' report={auditReport} verify={audit} enabled={!disabled && Boolean(view.steamBuild) && !view.steamIssue} />}
            <ModLoader loader={loader} disabled={disabled}
              select={() => { void run(message('me3 실행 파일 선택 중', 'Selecting the me3 executable'), async () => {
                const selected = await native<LoaderStatus | null>('select_loader')
                if (selected) { setLoader(selected); setFeedback({ error: false, message: message('me3 실행 파일을 연결했습니다.', 'The me3 executable is connected.') }) }
              }) }}
              detect={() => { void run(message('me3 확인 중', 'Detecting me3'), detectLoader) }}
              download={() => { void run(message('공식 다운로드 페이지 여는 중', 'Opening the official downloads page'), () => native('open_loader_download'), message('설치 후 자동 찾기를 누르세요.', 'After installing me3, choose Auto-Detect.')) }} />
          </>}
        </div></main>
      </div>
      {question && <ConfirmDialog question={question} close={() => setQuestion(null)} />}
      {modReview && <ModConflictDialog review={modReview.review} vanilla={view?.activeId === 'vanilla'} close={() => setModReview(null)}
        apply={(name, keepExisting) => {
          setModReview(null)
          mutate('profile_from_mods', { source: modReview.source, name, reviewToken: modReview.review.token, keepExisting },
            message('선택한 파일로 프로필 생성 중', 'Creating a profile from the selected files'), message('새 프로필을 보관함에 추가했습니다.', 'The new profile has been added to your library.'))
        }} />}
    </div>
  )
}
