import type { Language, Translate } from '@/locale'
import { useLocale } from '@/locale'
import type { ReactElement } from 'react'
import { useState } from 'react'
import { TbArchive, TbDeviceFloppy, TbFile, TbFolderDown, TbFolderUp, TbHistory, TbRefresh, TbUsers } from 'react-icons/tb'
import type { Backup, SaveFile, Slot } from '@/native'
import { formatBytes } from '@/native'
import { translateNative } from '@/nativeMessages'
import { Panel } from './Controls'
import AutoBackup, { PeriodicPin } from './AutoBackup'

export function backupDate(timestamp: number, lang: Language = 'ko'): string {
  return new Intl.DateTimeFormat(lang === 'en' ? 'en-US' : 'ko-KR', {
    year: 'numeric', month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit', second: '2-digit',
  }).format(new Date(timestamp * 1000))
}

function backupTitle(backup: Backup, lang: Language): string {
  if (!backup.label) return backupDate(backup.created, lang)
  const defaultLabels: Partial<Record<Backup['reason'], string>> = {
    beforeRestore: '세이브 복구 전', beforeSwitch: '프로필 전환 전', periodic: '주기 백업',
  }
  return backup.label === defaultLabels[backup.reason] ? translateNative(backup.label, lang) : backup.label
}

function fileFormat(name: string): string {
  return name.toLowerCase().replace(/\.bak$/, '').match(/\.[^.]+$/)?.[0] ?? ''
}

export function fileKey(file: SaveFile): string {
  return `${file.account}/${file.name}/${file.modified}/${file.bytes}`
}

function playtime(seconds: number, t: Translate): string {
  const hours = Math.floor(seconds / 3600)
  const minutes = Math.floor((seconds % 3600) / 60)
  return hours ? t(`${hours}시간 ${minutes}분`, `${hours}h ${minutes}m`) : t(`${minutes}분`, `${minutes}m`)
}

export interface Peek {
  key: string
  slots: Slot[]
}

interface Props {
  backups: Backup[] | null
  inventory: SaveFile[] | null
  peek: Peek | null
  disabled: boolean
  make: (label: string, account: string | null, format: string | null) => void
  restore: (backup: Backup) => void
  exportBackup: (backup: Backup) => void
  importBackup: () => void
  load: () => void
  onBackupsChanged: () => Promise<void>
  viewSlots: (file: SaveFile) => void
}

export default function Saves({ backups, inventory, peek, disabled, make, restore, exportBackup, importBackup, load, onBackupsChanged, viewSlots }: Props): ReactElement {
  const { lang, t } = useLocale()
  const reasons: Record<Backup['reason'], string> = {
    manual: t('수동 백업', 'Manual backup'), beforeRestore: t('복원 전 자동 백업', 'Automatic backup before restore'), beforeSwitch: t('전환 전 자동 백업', 'Automatic backup before switch'), periodic: t('정기 백업', 'Scheduled backup'),
  }
  const [account, setAccount] = useState('')
  const [format, setFormat] = useState('')
  const [label, setLabel] = useState('')
  const accounts = [...new Set(inventory?.map(file => file.account) ?? [])].sort()
  const formats = [...new Set(inventory?.map(file => fileFormat(file.name)) ?? [])].filter(Boolean).sort()
  const currentAccount = accounts.includes(account) ? account : ''
  const currentFormat = formats.includes(format) ? format : ''
  const selected = (inventory ?? []).filter(file => (!currentAccount || currentAccount === file.account) &&
    (!currentFormat || currentFormat === fileFormat(file.name)))
  return (
    <>
      <div className='page-title'><div><h1>{t('세이브 관리', 'Save Manager')}</h1><p>{t('계정과 파일 형식을 선택해 진행 상황을 살펴보고 백업하세요.', 'Choose an account and file type to view character progress and back up saves.')}</p></div>
        <button className='action small' onClick={load} disabled={disabled}><TbRefresh />{t('목록 갱신', 'Refresh List')}</button>
      </div>
      <Panel title={t('현재 세이브', 'Current Saves')} extra={<span className='count'>{selected.length}{t('개 파일', selected.length === 1 ? ' file' : ' files')}</span>}>
        <div className='save-filters'>
          <label className='field'>{t('Steam 계정', 'Steam account')}<select value={currentAccount} onChange={e => setAccount(e.target.value)}>
            <option value=''>{t('전체 계정', 'All accounts')}</option>{accounts.map(id => <option key={id}>{id}</option>)}</select></label>
          <label className='field'>{t('파일 형식', 'File type')}<select value={currentFormat} onChange={e => setFormat(e.target.value)}>
            <option value=''>{t('전체 형식', 'All types')}</option>{formats.map(ext => <option key={ext}>{ext}</option>)}</select></label>
        </div>
        <div className='inventory'>
          {selected.length ? selected.map(file => {
            const key = fileKey(file)
            const canPreview = /\.(sl2|co2)$/i.test(file.name)
            return <div key={key}>
              <div className='row'><span className='row-icon'><TbFile /></span>
                <div className='row-text'><strong>{file.name}</strong><span>{t('계정 ', 'Account ')}{file.account} · {formatBytes(file.bytes)} · {backupDate(file.modified, lang)}</span></div>
                {canPreview && <button className='action small' disabled={disabled} onClick={() => viewSlots(file)}><TbUsers />{t('캐릭터 슬롯', 'Character Slots')}</button>}
              </div>
              {peek?.key === key && <div className='peek' role='status'>
                {peek.slots.length ? <table><caption>{t('캐릭터 슬롯', 'Character Slots')}</caption><thead><tr><th>{t('슬롯', 'Slot')}</th><th>{t('캐릭터', 'Character')}</th><th>{t('레벨', 'Level')}</th><th>{t('플레이 시간', 'Playtime')}</th></tr></thead>
                  <tbody>{peek.slots.map(slot => <tr key={slot.slot}><td>{slot.slot}</td><td>{slot.name}</td><td>{slot.level}</td><td>{playtime(slot.seconds, t)}</td></tr>)}</tbody></table>
                  : <span>{t('사용 중인 캐릭터 슬롯이 없습니다.', 'No occupied character slots.')}</span>}
              </div>}
            </div>
          }) : <div className='empty compact'><TbArchive /><strong>{inventory ? t('선택한 조건의 세이브가 없습니다', 'No saves match these filters') : t('세이브 목록을 불러와 주세요', 'Load the save list to view your files')}</strong></div>}
        </div>
        <form className='backup-form' onSubmit={e => { e.preventDefault(); make(label.trim(), currentAccount || null, currentFormat || null) }}>
          <label className='field'>{t('백업 이름 ', 'Backup name ')}<small>{t('(선택)', '(optional)')}</small><input disabled={disabled} maxLength={64} placeholder={t('예: DLC 진입 전', 'e.g. Before the DLC')} value={label} onChange={e => setLabel(e.target.value)} /></label>
          <button type='submit' className='action primary' disabled={disabled || !selected.length}><TbDeviceFloppy />{t('선택한 파일 백업', 'Back Up Selected Files')}</button>
        </form>
        <p className='footnote'>{t('백업 대상 ', 'Selected: ')}{selected.length}{t('개 · ', ' · ')}{formatBytes(selected.reduce((sum, f) => sum + f.bytes, 0))}{t('. 같은 형식의 .bak 파일도 함께 보관됩니다.', '. Matching .bak files are included.')}</p>
      </Panel>
      <AutoBackup disabled={disabled} onBackupsChanged={onBackupsChanged} />
      <Panel title={t('백업 목록', 'Backups') + (backups ? t(` · ${backups.length}개`, ` · ${backups.length}`) : '')} extra={<button className='action small' disabled={disabled}
        onClick={importBackup}><TbFolderDown />{t('백업 가져오기', 'Import Backup')}</button>}>
        {backups?.length ? <div className='rows'>{[...backups].sort((a, b) => b.created - a.created).map(backup => (
          <div key={backup.id} className='row'>
            <span className='row-icon'><TbArchive /></span>
            <div className='row-text'><strong>{backupTitle(backup, lang)}</strong>
              <span>{backup.label && `${backupDate(backup.created, lang)} · `}{reasons[backup.reason] ?? reasons.manual} · {backup.files}{t('개 · ', backup.files === 1 ? ' file · ' : ' files · ')}{formatBytes(backup.bytes)}</span>
              {!!backup.accounts?.length && <span>{t('계정 ', 'Account ')}{backup.accounts.join(', ')} · {backup.formats.join(', ')}</span>}
            </div>
            {backup.reason === 'periodic' && <PeriodicPin backup={backup} disabled={disabled} onBackupsChanged={onBackupsChanged} />}
            <button className='icon-btn' disabled={disabled} title={t('백업 내보내기', 'Export Backup')} aria-label={backupTitle(backup, lang) + t(' 내보내기', ' – Export')}
              onClick={() => exportBackup(backup)}><TbFolderUp size={18} /></button>
            <button className='action small' disabled={disabled} onClick={() => restore(backup)}><TbHistory />{t('복원', 'Restore')}</button>
          </div>
        ))}</div> : <div className='empty compact'><TbArchive aria-hidden /><strong>{backups ? t('아직 저장된 백업이 없습니다', 'No backups yet') : t('백업 목록을 불러와 주세요', 'Load the backup list')}</strong></div>}
      </Panel>
      <p className='footnote'>{t('모드 플레이에 사용한 세이브는 별도로 보관하고, 온라인에는 바닐라 플레이용 세이브를 사용하세요.', 'Keep saves used for modded play separate. Use your vanilla saves when playing online.')}</p>
    </>
  )
}
