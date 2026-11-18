import { useLocale } from '@/locale'
import type { ReactElement } from 'react'
import { TbArrowRight, TbBrandSteam, TbFolderOpen, TbSearch } from 'react-icons/tb'
import { Panel } from './Controls'
import type { View } from '@/native'

interface Props {
  view: View | null
  path: string
  setPath: (value: string) => void
  disabled: boolean
  browse: () => void
  detect: () => void
  connect: () => void
  onboard: () => void
  baseline: () => void
  steam: () => void
  recover: () => void
  undo: (id: string) => void
  folder: () => void
}

export default function Settings(props: Props): ReactElement {
  const { t } = useLocale()
  const { view, disabled } = props
  return (
    <>
      <div className='page-title'><div><h1>{t('설치 설정', 'Settings')}</h1><p>{t('게임 폴더를 연결하고 바닐라 기준 파일을 관리합니다.', 'Connect your game folder and manage the vanilla baseline.')}</p></div></div>
      <Panel title={t('게임 설치 위치', 'Game Installation')}>
        <form className='path-form' onSubmit={e => { e.preventDefault(); props.connect() }}>
          <label className='field' htmlFor='game-path'>{t('ELDEN RING의 Game 폴더', 'Elden Ring Game folder')}</label>
          <div className='path-input'><input id='game-path' spellCheck={false} placeholder={'…\\ELDEN RING\\Game'} value={props.path}
            onChange={e => props.setPath(e.target.value)} disabled={disabled} />
            <button className='action' type='submit' disabled={disabled || !props.path.trim()}>{t('연결', 'Connect')}</button></div>
        </form>
        <div className='actions'><button className='action small' onClick={props.browse} disabled={disabled}><TbFolderOpen />{t('폴더 선택', 'Choose Folder')}</button>
          <button className='action small' onClick={props.detect} disabled={disabled}><TbSearch />{t('Steam에서 찾기', 'Find in Steam')}</button></div>
        {view && <dl className='facts'><dt>{t('연결된 폴더', 'Connected folder')}</dt><dd>{view.gamePath}</dd>
          {!!view.vaultPath && <><dt>{t('프로필 보관 위치', 'Profile storage')}</dt><dd>{view.vaultPath}</dd></>}
          <dt>{t('현재 Steam 빌드', 'Current Steam build')}</dt><dd>{view.steamBuild ?? t('확인되지 않음', 'Unknown')}</dd>
          {!!view.baselineBuild && <><dt>{t('기준본 빌드', 'Baseline build')}</dt><dd>{view.baselineBuild}</dd></>}
        </dl>}
      </Panel>
      {view?.phase === 'unmanaged' && <Panel title={t('처음 등록하기', 'Initial Setup')}>
        <div className='choice'><div><strong>{t('깨끗한 바닐라를 설치했을 때', 'Clean vanilla installation')}</strong><p>{t('원본 파일을 검사해 모드 프로필이 함께 사용할 기준으로 등록합니다.', 'Verify the original files and register a shared baseline for your mod profiles.')}</p></div>
          <button className='action small' disabled={disabled || Boolean(view.steamIssue)} onClick={props.baseline}>{t('원본 등록', 'Register Baseline')}<TbArrowRight /></button></div>
        {view.steamBuild && <div className='choice'><div><strong>{t('이미 Game 안에 모드를 설치했을 때', 'Mods already installed in the Game folder')}</strong><p>{t('Steam 원본과 다른 파일만 분리한 뒤 필요한 원본을 복구합니다.', 'Separate files that differ from Steam’s originals, then restore the original game files.')}</p></div>
          <button className='action small' disabled={disabled} onClick={props.onboard}>{t('설치된 모드 분리', 'Separate Installed Mods')}<TbArrowRight /></button></div>}
        {view.steamBuild && <button className='action small' onClick={props.steam} disabled={disabled}><TbBrandSteam />{t('Steam 파일 검사', 'Verify Files in Steam')}</button>}
      </Panel>}
      {view?.phase === 'awaitingVanilla' && <Panel title={t('바닐라 원본 복구', 'Recover Vanilla Files')}>
        <ol className='steps'><li>{t('Steam 파일 검사를 열어 누락되거나 변경된 원본을 복구하세요.', 'Open Steam file verification to recover missing or changed game files.')}</li>
          <li>{t('검사와 다운로드가 끝나면 복구한 원본을 등록하세요.', 'After verification and downloads finish, register the recovered baseline.')}</li></ol>
        <div className='actions'><button className='action' onClick={props.steam} disabled={disabled}><TbBrandSteam />{t('Steam 파일 검사', 'Verify Files in Steam')}</button>
          <button className='action primary' onClick={props.baseline} disabled={disabled}>{t('받은 원본 등록', 'Register Recovered Files')}<TbArrowRight /></button></div>
        {view.profiles.length === 1 && <div className='actions'><button className='action quiet small' disabled={disabled}
          onClick={() => props.undo(view.profiles[0].id)}>{t('보관 취소 · 기존 설치 되돌리기', 'Undo Archive and Restore Installation')}</button></div>}
      </Panel>}
      {view?.phase === 'recovery' && <Panel title={t('중단된 작업 복구', 'Recover an Interrupted Operation')}><p>{t('파일 배치를 확인하고 중단된 전환을 마무리합니다.', 'Check file locations and finish the interrupted profile switch.')}</p>
        <button className='action primary' disabled={disabled} onClick={props.recover}>{t('전환 복구', 'Recover Switch')}</button></Panel>}
      {view?.phase === 'ready' && <Panel title={t('바닐라 유지 관리', 'Vanilla Maintenance')}>
        <p>{view.activeId === 'vanilla' ? t('Steam 업데이트 후 검사와 다운로드가 끝나면 새 빌드의 기준본을 등록하세요.', 'After Steam updates the game and finishes verification, register the new build as your baseline.') :
          t('Steam 파일 검사와 바닐라 원본 등록은 바닐라로 전환한 뒤 진행하세요.', 'Switch to vanilla before verifying files in Steam or registering a new vanilla baseline.')}</p>
        <div className='actions'><button className='action' disabled={disabled || view.activeId !== 'vanilla'} onClick={props.steam}><TbBrandSteam />{t('Steam 파일 검사', 'Verify Files in Steam')}</button>
          <button className='action' disabled={disabled || view.activeId !== 'vanilla'} onClick={props.baseline}>{t('원본 다시 등록', 'Register Baseline Again')}</button>
          <button className='action' disabled={disabled} onClick={props.folder}><TbFolderOpen />{t('게임 폴더', 'Game Folder')}</button></div>
        {view.activeId === 'vanilla' && view.steamBuild && <details className='differences'><summary>{t('모드 파일이 바닐라에 섞였을 때', 'Mod files mixed into your vanilla installation?')}</summary>
          <p>{t('Steam 원본과 다른 파일을 분리해 모드 프로필로 보관한 뒤 바닐라를 복구하세요.', 'Archive files that differ from Steam’s originals as a mod profile, then restore the vanilla game files.')}</p>
          <button className='action small' disabled={disabled} onClick={props.onboard}>{t('설치된 모드 분리', 'Separate Installed Mods')}</button></details>}
      </Panel>}
    </>
  )
}
