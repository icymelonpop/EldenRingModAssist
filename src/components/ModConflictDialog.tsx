import { translateNative } from '@/nativeMessages'
import { useLocale } from '@/locale'
import { useEffect, useId, useMemo, useRef, useState } from 'react'
import type { ReactElement } from 'react'
import { TbFiles, TbX } from 'react-icons/tb'
import { formatBytes, native } from '@/native'
import styles from '../styles/conflict-review.module.css'

export interface ModPreview {
  token: string
  baseName: string
  added: string[]
  identical: string[]
  conflicts: { path: string; existingBytes: number; incomingBytes: number }[]
}

interface Props {
  review: ModPreview
  vanilla: boolean
  close: () => void
  apply: (name: string, keepExisting: string[]) => void
}

const sections = ['충돌', '추가', '동일'] as const
type Section = typeof sections[number]
const pageSize = 25

export default function ModConflictDialog({ review, vanilla, close, apply }: Props): ReactElement {
  const { lang, t } = useLocale()
  const profileName = vanilla ? t('바닐라', 'Vanilla') : review.baseName
  const dialog = useRef<HTMLDialogElement>(null)
  const title = useId()
  const [name, setName] = useState('')
  const [checked, setChecked] = useState(false)
  const [keep, setKeep] = useState<Set<string>>(new Set())
  const [section, setSection] = useState<Section>(review.conflicts.length ? '충돌' : review.added.length ? '추가' : '동일')
  const [page, setPage] = useState(0)
  const [search, setSearch] = useState('')
  const [linkError, setLinkError] = useState('')
  const sizes = useMemo(() => new Map(review.conflicts.map(file => [file.path, file])), [review])
  const paths = useMemo(() => {
    const all = section === '충돌' ? [...sizes.keys()] : section === '추가' ? review.added : review.identical
    const query = search.trim().toLowerCase()
    return query ? all.filter(path => path.toLowerCase().includes(query)) : all
  }, [section, sizes, review, search])
  const sectionNames: Record<Section, string> = {
    충돌: t('충돌', 'Conflicts'), 추가: t('추가', 'Added'), 동일: t('동일', 'Identical'),
  }
  const counts = [review.conflicts.length, review.added.length, review.identical.length]
  const pages = Math.ceil(paths.length / pageSize)
  const regulation = review.conflicts.some(file => file.path.toLowerCase() === 'regulation.bin')

  useEffect(() => {
    const el = dialog.current
    el?.showModal()
    return () => el?.close()
  }, [])

  function keepFile(path: string, value: boolean): void {
    setChecked(false)
    setKeep(previous => {
      const next = new Set(previous)
      if (value) next.add(path)
      else next.delete(path)
      return next
    })
  }

  return <dialog ref={dialog} className={`dialog ${styles.dialog}`} aria-labelledby={title}
    onCancel={event => { event.preventDefault(); close() }}>
    <form onSubmit={e => { e.preventDefault(); if (name.trim() && (!review.conflicts.length || checked)) apply(name.trim(), [...keep]) }}>
      <div className='dialog-top'><span className='icon-tile'><TbFiles size={23} aria-hidden /></span>
        <button type='button' className='icon-btn' aria-label={t('닫기', 'Close')} onClick={close}><TbX size={20} /></button></div>
      <h2 id={title}>{t('모드 파일 비교', 'Compare Mod Files')}</h2>
      <p>{t(`「${profileName}」에 모드 폴더를 더할 때 새 프로필에 담길 파일을 확인하세요.`, `Review the files that will be added to “${profileName}” in the new profile.`)}</p>
      <div className={styles.tabs} aria-label={t('비교 결과', 'Comparison results')}>
        {sections.map((label, i) => <button type='button' key={label} className='action small'
          aria-pressed={section === label} onClick={() => { setSection(label); setSearch(''); setPage(0) }}>
          {sectionNames[label]} {counts[i].toLocaleString(lang)}</button>)}
      </div>
      {regulation && <div className={styles.regulation}><p className='callout'>{t('두 모드가 서로 다른 regulation.bin을 사용합니다. 아래에서 사용할 파일 하나를 고르세요. 두 모드의 수치 변경을 모두 쓰려면 제작자의 호환 패치를 사용하거나 Smithbox로 병합한 파일을 모드 폴더로 가져오세요.', 'Both mods use different regulation.bin files. Choose which file to use. To combine the changes from both mods, use the author’s compatibility patch or merge the files in Smithbox and place the result in the mod folder.')}</p>
        <button type='button' className='action small' onClick={() => {
          setLinkError('')
          void native('open_smithbox').catch(error => setLinkError(String(error)))
        }}>{t('Smithbox 공식 페이지', 'Official Smithbox Page')}</button>
        {linkError && <p role='alert'>{translateNative(linkError, lang)}</p>}
      </div>}
      {section === '동일' && <p className={styles.note}>{t('경로와 내용이 같은 파일은 한 벌만 보관합니다.', 'Only one copy is kept when both the path and the contents match.')}</p>}
      {section === '충돌' && review.conflicts.length > 1 && <div className={styles.bulk}>
        <button type='button' className='action small' onClick={() => { setKeep(new Set(sizes.keys())); setChecked(false) }}>{t('모두 기존 파일 사용', 'Use All Existing Files')}</button>
        <button type='button' className='action small' onClick={() => { setKeep(new Set()); setChecked(false) }}>{t('모두 새 파일 사용', 'Use All New Files')}</button>
      </div>}
      <label className='field'>{t('파일 경로 검색', 'Search file paths')}<input type='search' value={search} onChange={e => { setSearch(e.target.value); setPage(0) }} /></label>
      <ul className={styles.files} aria-label={sectionNames[section] + t(' 파일', ' files')}>
        {paths.slice(page * pageSize, (page + 1) * pageSize).map(path => {
          const collision = sizes.get(path)
          return <li key={path}><div><strong>{path}</strong>{collision && <small>{t('기존 ', 'Existing: ')}{formatBytes(collision.existingBytes)}{t(' · 새 파일 ', ' · New: ')}{formatBytes(collision.incomingBytes)}</small>}</div>
            {collision && <select aria-label={path + t('에 사용할 파일', ' – File to use')} value={keep.has(path) ? 'keep' : 'replace'}
              onChange={e => keepFile(path, e.target.value === 'keep')}><option value='replace'>{t('새 파일 사용', 'Use new file')}</option><option value='keep'>{t('기존 파일 유지', 'Keep existing file')}</option></select>}
          </li>
        })}
        {!paths.length && <li>{search ? t('일치하는 경로가 없습니다.', 'No matching paths.') : t('해당하는 파일이 없습니다.', 'No files in this category.')}</li>}
      </ul>
      {pages > 1 && <div className={styles.pager}><button type='button' className='action small' disabled={!page}
        onClick={() => setPage(p => p - 1)}>{t('이전', 'Previous')}</button><span>{page + 1} / {pages}</span>
        <button type='button' className='action small' disabled={page + 1 === pages} onClick={() => setPage(p => p + 1)}>{t('다음', 'Next')}</button></div>}
      <label className='field'>{t('새 프로필 이름', 'New profile name')}<input required maxLength={64} value={name} onChange={e => setName(e.target.value)} /></label>
      {review.conflicts.length > 0 && <label className={`check ${styles.confirm}`}><input type='checkbox' checked={checked} onChange={e => setChecked(e.target.checked)} />{t('기존 파일 ', 'Existing files: ')}{keep.size}{t('개와 새 파일 ', ' · New files: ')}{review.conflicts.length - keep.size}{t('개를 사용하도록 충돌 항목을 확인했습니다.', '. I have reviewed the file conflicts.')}</label>}
      <div className='actions dialog-actions'><button type='button' className='action' onClick={close}>{t('취소', 'Cancel')}</button>
        <button type='submit' className='action primary' disabled={!name.trim() || (review.conflicts.length > 0 && !checked)}>{t('프로필 만들기', 'Create Profile')}</button></div>
    </form>
  </dialog>
}
