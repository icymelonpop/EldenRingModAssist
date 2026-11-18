import { useLocale } from '@/locale'
import type { ReactElement, ReactNode } from 'react'
import { useEffect, useId, useRef, useState } from 'react'
import { TbAlertTriangle, TbX } from 'react-icons/tb'

export interface Question {
  title: string
  message: string
  button: string
  input?: { label: string; initial: string }
  check?: string
  action: (value: string) => void
}

interface Props {
  question: Question
  close: () => void
}

export function ConfirmDialog({ question, close }: Props): ReactElement {
  const { t } = useLocale()
  const dialog = useRef<HTMLDialogElement>(null)
  const title = useId()
  const [value, setValue] = useState(question.input?.initial ?? '')
  const [checked, setChecked] = useState(false)
  useEffect(() => {
    const el = dialog.current
    if (!el?.open) el?.showModal()
    return () => { el?.close() }
  }, [])
  return (
    <dialog ref={dialog} aria-labelledby={title} className='dialog' onCancel={event => { event.preventDefault(); close() }}>
      <form onSubmit={event => { event.preventDefault(); close(); question.action(value.trim()) }}>
        <div className='dialog-top'><span className='icon-tile'><TbAlertTriangle size={23} aria-hidden /></span>
          <button type='button' onClick={close} className='icon-btn' aria-label={t('닫기', 'Close')}><TbX size={21} /></button>
        </div>
        <h2 id={title}>{question.title}</h2>
        <p>{question.message}</p>
        {question.input && <label className='field'>{question.input.label}
          <input autoFocus maxLength={64} required value={value} onChange={e => setValue(e.target.value)} />
        </label>}
        {question.check && <label className='check'><input type='checkbox' checked={checked}
          onChange={e => setChecked(e.target.checked)} />{question.check}</label>}
        <div className='actions dialog-actions'>
          <button type='button' className='action' onClick={close}>{t('취소', 'Cancel')}</button>
          <button className='action primary' disabled={Boolean(question.input && !value.trim()) || Boolean(question.check && !checked)}>{question.button}</button>
        </div>
      </form>
    </dialog>
  )
}

interface PanelProps {
  title: string
  extra?: ReactNode
  children: ReactNode
}

export function Panel({ title, extra, children }: PanelProps): ReactElement {
  return <section className='panel'><div className='panel-head'><h2>{title}</h2>{extra}</div>{children}</section>
}
