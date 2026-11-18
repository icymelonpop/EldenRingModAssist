import { useLocale } from '@/locale'
import type { ReactElement } from 'react'
import {
  TbArrowRight, TbFolderOpen, TbFolderUp, TbFolders, TbPencil, TbPlayerPlay, TbPlus, TbShield,
} from 'react-icons/tb'
import { activeName, canLaunchPackage, formatBytes } from '@/native'
import type { Profile, View } from '@/native'
import { Panel } from './Controls'

interface Props {
  view: View | null
  disabled: boolean
  create: () => void
  importProfile: () => void
  addMods: () => void
  addNative: () => void
  activate: (id: string, name: string) => void
  rename: (id: string, name: string) => void
  release: (id: string, name: string) => void
  convert: (id: string, name: string) => void
  autoBackup: boolean
  setAutoBackup: (value: boolean) => void
  folder: (id: string) => void
  settings: () => void
  launch: () => void
  loaderInstalled: boolean
  loaderSetup: () => void
}

export default function Profiles(props: Props): ReactElement {
  const { t } = useLocale()
  const { view, disabled } = props
  const ready = view?.phase === 'ready'
  const profiles: Profile[] = [
    ...(ready ? [{ id: 'vanilla', name: t('바닐라', 'Vanilla'), build: view.baselineBuild }] : []),
    ...(view?.profiles ?? []),
  ]
  return (
    <>
      <div className='page-title'><div><h1>{t('모드 프로필', 'Mod Profiles')}</h1><p>{t('하나의 게임 설치에서 모드 조합을 바꿔 플레이하세요.', 'Switch between mod setups using a single game installation.')}</p></div>
        <button type='button' className='action primary' disabled={disabled || !ready} onClick={props.addMods}><TbPlus />{t('모드 폴더로 프로필 만들기', 'Create Profile from Mod Folder')}</button>
      </div>
      <Panel title={t('프로필 보관함', 'Profile Library')} extra={profiles.length > 0 && <span className='count'>{profiles.length}{t('개', '')}</span>}>
        {!profiles.length && <div className='empty'><TbFolders /><strong>{t('바닐라 원본을 먼저 등록하세요', 'Register the vanilla baseline first')}</strong>
          <button className='action small' onClick={props.settings}>{t('설치 설정 열기', 'Open Settings')}<TbArrowRight /></button></div>}
        {profiles.length > 0 && <div className='rows'>{profiles.map(profile => {
          const isVanilla = profile.id === 'vanilla'
          const packed = profile.kind === 'package'
          const active = ready && view?.activeId === profile.id
          const stale = !isVanilla && Boolean(profile.build && view?.steamBuild && profile.build !== view.steamBuild)
          const kind = isVanilla ? t('Steam 원본', 'Original Steam files') : packed ? t('경량 프로필 · me3', 'Lightweight profile · me3') : t('이전 전체 설치본', 'Legacy full installation')
          return <div className={`row profile-row ${active ? 'active-row' : ''}`} key={profile.id}>
            <span className='row-icon'>{isVanilla ? <TbShield /> : <TbFolders />}</span>
            <div className='row-text'><strong>{profile.name}</strong><span>{isVanilla && !view?.baselineBuild ? t('로컬 원본', 'Local baseline') : kind}
              {packed && typeof profile.bytes === 'number' ? t(` · 보관 용량 ${formatBytes(profile.bytes)}`, ` · Stored: ${formatBytes(profile.bytes)}`) : ''}
              {profile.build ? t(` · 빌드 ${profile.build}`, ` · Build ${profile.build}`) : ''}{stale ? t(' · 이전 빌드', ' · Older build') : ''}</span>
              {Boolean(profile.archivedFiles) && <span>{t('별도 설치 필요 ', 'Files needing separate installation: ')}{profile.archivedFiles}{t('개 · 프로필의 preserved 폴더에 보관 중', ' · Stored in the profile’s preserved folder')}</span>}
            </div>
            <div className='profile-actions'>
              {ready && packed && <button type='button' className='icon-btn' disabled={disabled} aria-label={profile.name + t(' 프로필 폴더 열기', ' – Open profile folder')}
                title={t('프로필 폴더 열기', 'Open Profile Folder')} onClick={() => props.folder(profile.id)}><TbFolderOpen size={16} /></button>}
              {!isVanilla && ready && <button type='button' className='icon-btn' disabled={disabled} aria-label={profile.name + t(' 이름 변경', ' – Rename')}
                title={t('프로필 이름 변경', 'Rename Profile')} onClick={() => props.rename(profile.id, profile.name)}><TbPencil size={16} /></button>}
              {!isVanilla && !active && ready && <button type='button' className='icon-btn' disabled={disabled} aria-label={profile.name + t(' 보관 해제', ' – Remove from library')}
                title={t('보관 해제', 'Remove from Library')} onClick={() => props.release(profile.id, profile.name)}><TbFolderUp size={16} /></button>}
              {!isVanilla && !packed && ready && <button type='button' className='action small' disabled={disabled || stale || view.stale}
                onClick={() => props.convert(profile.id, profile.name)}>{t('경량 사본 만들기', 'Create Lightweight Copy')}</button>}
              {active ? <span className='chip'><i />{t('사용 중', 'Active')}</span> :
                <button className='action small' disabled={disabled || !ready || stale || (!isVanilla && view?.stale)} onClick={() => props.activate(profile.id, profile.name)}>{t('전환', 'Switch')}<TbArrowRight /></button>}
              {active && packed && (props.loaderInstalled ?
                <button type='button' className='action small primary' disabled={disabled || !canLaunchPackage(view)} onClick={props.launch}><TbPlayerPlay />{t('모드 실행', 'Launch Mods')}</button> :
                <button type='button' className='action small' disabled={disabled} onClick={props.loaderSetup}>{t('me3 연결', 'Connect me3')}</button>)}
            </div>
          </div>
        })}</div>}
        <label className='check auto-backup'><input type='checkbox' checked={props.autoBackup} disabled={disabled}
          onChange={e => props.setAutoBackup(e.target.checked)} />{t('프로필 전환 직전 세이브 자동 백업', 'Back up saves automatically before switching profiles')}</label>
      </Panel>
      <Panel title={t('프로필 만들기', 'Create a Profile')}>
        <div className='choice'><div><strong>{t('DLL 모드 추가', 'Add a DLL Mod')}</strong><p>{t('me3 호환 모드의 폴더를 고른 뒤 실행할 DLL을 지정합니다. 전용 설치기가 있는 모드는 제작자의 안내를 확인하세요.', 'Select an me3-compatible mod folder, then choose the DLL to load. For mods with their own installer, follow the author’s instructions.')}</p></div>
          <button className='action small' disabled={disabled || !ready} onClick={props.addNative}>{t('DLL 모드로 프로필 만들기', 'Create Profile from DLL Mod')}</button></div>
        <div className='choice'><div><strong>{t('현재 조합에서 시작', 'Start with the Current Setup')}</strong>
          <p>{view?.activeId === 'vanilla' ? t('빈 프로필을 만들어 모드 파일을 담을 수 있습니다.', 'Create an empty profile and add mod files to it.') : t(`「${activeName(view)}」의 모드 파일로 새 프로필을 만듭니다.`, `Create a profile with the mod files from “${activeName(view, 'en')}”.`)}</p></div>
          <button className='action small' disabled={disabled || !ready} onClick={props.create}>{view?.activeId === 'vanilla' ? t('빈 프로필 만들기', 'Create Empty Profile') : t('프로필 복제', 'Duplicate Profile')}</button></div>
        <div className='choice'><div><strong>{t('별도 설치본에서 가져오기', 'Import from Another Installation')}</strong><p>{t('기존 Game 폴더를 바닐라와 비교해 추가·변경된 모드 파일을 추출합니다.', 'Compare an existing Game folder with vanilla to extract added and modified mod files.')}</p></div>
          <button className='action small' disabled={disabled || !ready} onClick={props.importProfile}>{t('설치본 가져오기', 'Import Installation')}</button></div>
      </Panel>
    </>
  )
}
