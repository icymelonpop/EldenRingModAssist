import { translateNative } from '@/nativeMessages'
import { useLocale } from '@/locale'
import { useEffect, useRef, useState } from 'react'
import type { FormEvent, ReactElement } from 'react'
import { TbPin, TbPinnedOff } from 'react-icons/tb'
import { hasDesktop, native } from '@/native'
import { Panel } from './Controls'

interface Settings {
  enabled: boolean
  intervalMinutes: number
  keepCount: number
}

interface Status {
  settings: Settings
  lastChecked: number
  lastBackup: number
  lastBackupId: string | null
  nextCheck: number
  message: string
}

interface Props {
  disabled: boolean
  onBackupsChanged: () => Promise<void> | void
}

const defaultSettings: Settings = { enabled: false, intervalMinutes: 5, keepCount: 12 }

export default function AutoBackup({ disabled, onBackupsChanged }: Props): ReactElement {
  const { lang, t } = useLocale()
  const [state, setState] = useState<Status | null>(null)
  const [draft, setDraft] = useState(defaultSettings)
  const [dirty, setDirty] = useState(false)
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState('')
  const refresh = useRef(onBackupsChanged)
  const editVersion = useRef(0)
  const prior = useRef<Status | null>(null)
  useEffect(() => { refresh.current = onBackupsChanged }, [onBackupsChanged])

  useEffect(() => {
    if (!hasDesktop()) return
    let mounted = true
    let pending = false
    async function poll(): Promise<void> {
      if (pending) return
      pending = true
      const version = editVersion.current
      try {
        const next = await native<Status>('periodic_status')
        if (!mounted || version !== editVersion.current) return
        const previous = prior.current
        setState(next)
        if (previous && previous.lastChecked !== next.lastChecked) await refresh.current()
        if (!mounted) return
        prior.current = next
        setError('')
      } catch (e) {
        if (mounted) setError(String(e))
      } finally { pending = false }
    }
    void poll()
    const timer = window.setInterval(() => { void poll() }, 5000)
    return () => { mounted = false; window.clearInterval(timer) }
  }, [])

  useEffect(() => {
    if (!dirty && state) setDraft(state.settings)
  }, [state, dirty])

  function change(next: Partial<Settings>): void {
    setDraft(current => ({ ...current, ...next }))
    setDirty(true)
  }

  async function save(e: FormEvent): Promise<void> {
    e.preventDefault()
    if (saving || !state) return
    editVersion.current++
    setSaving(true)
    try {
      const updated = await native<Status>('set_periodic', { settings: draft })
      editVersion.current++
      prior.current = updated
      setState(updated)
      setDirty(false)
      setError('')
    } catch (failure) { setError(String(failure)) }
    finally { setSaving(false) }
  }

  const locked = disabled || saving || !state
  return (
    <Panel title={t('주기 자동 백업', 'Scheduled Backups')} extra={<span className='count'>{state ? (state.settings.enabled ? t('작동 중', 'On') : t('꺼짐', 'Off')) : t('확인 중', 'Checking')}</span>}>
      <form onSubmit={save}>
        <label className='check'><input type='checkbox' checked={draft.enabled} disabled={locked}
          onChange={e => change({ enabled: e.target.checked })} />{t('앱이 열려 있는 동안 변경된 세이브 백업', 'Back up changed saves while the app is open')}</label>
        <div className='save-filters'>
          <label className='field'>{t('확인 간격 (분)', 'Check interval (minutes)')}<input type='number' min='1' max='120' required
            value={draft.intervalMinutes || ''} disabled={locked}
            onChange={e => change({ intervalMinutes: Number(e.target.value) })} /></label>
          <label className='field'>{t('보관 개수', 'Backups to keep')}<input type='number' min='1' max='100' required value={draft.keepCount || ''}
            disabled={locked} onChange={e => change({ keepCount: Number(e.target.value) })} /></label>
        </div>
        <div className='actions'><button className='action small' disabled={locked || !dirty}>{t('자동 백업 설정 저장', 'Save Backup Settings')}</button></div>
      </form>
      <p className='footnote'>{t('최소화해도 작동합니다. 보관 개수를 넘으면 오래된 주기 백업부터 정리하며, 고정한 백업은 계속 보관합니다.', 'Backups continue while the app is minimized. When the limit is reached, the oldest scheduled backups are removed first. Pinned backups are kept.')}</p>
      {state?.lastChecked ? <p className='footnote'>{new Date(state.lastChecked * 1000).toLocaleString(lang === 'en' ? 'en-US' : 'ko-KR')}{t(' 확인 · ', ' · ')}{translateNative(state.message, lang)}
        {state.lastBackup > 0 && <>{t(' · 최근 주기 백업 ', ' · Last scheduled backup: ')}{new Date(state.lastBackup * 1000).toLocaleString(lang === 'en' ? 'en-US' : 'ko-KR')}</>}</p> : null}
      {error && <p role='alert' className='footnote'>{translateNative(error, lang)}</p>}
    </Panel>
  )
}

export function PeriodicPin({ backup, disabled, onBackupsChanged }: Props & {
  backup: { id: string; pinned?: boolean }
}): ReactElement {
  const { lang, t } = useLocale()
  const [pending, setPending] = useState(false)
  const [error, setError] = useState('')
  async function toggle(): Promise<void> {
    if (pending) return
    setPending(true)
    try {
      await native('pin_periodic', { id: backup.id, pinned: !backup.pinned })
      await onBackupsChanged()
      setError('')
    } catch (failure) { setError(String(failure)) }
    finally { setPending(false) }
  }
  return <><button className='action small' aria-pressed={Boolean(backup.pinned)} disabled={pending || disabled}
    onClick={toggle}>{backup.pinned ? <TbPinnedOff /> : <TbPin />}{backup.pinned ? t('고정 해제', 'Unpin') : t('계속 보관', 'Keep Backup')}</button>
    {error && <span role='alert' className='footnote'>{translateNative(error, lang)}</span>}</>
}
