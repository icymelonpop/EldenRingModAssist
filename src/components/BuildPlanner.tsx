import { useEffect, useState } from 'react'
import type { ReactElement } from 'react'
import { TbCoins, TbEraser } from 'react-icons/tb'
import { attributes, englishAttributes, compareOrigins, emptyDraft, parseDraft, runesBetween, validNumber } from '@/buildPlanner'
import type { BuildDraft } from '@/buildPlanner'
import { useLocale } from '@/locale'
import { Panel } from './Controls'
import styles from '@/styles/planner.module.css'

const storageKey = 'elden-ring-mod-assist.build-plan.v1'
const counts = { ko: new Intl.NumberFormat('ko-KR'), en: new Intl.NumberFormat('en-US') }

export default function BuildPlanner(): ReactElement {
  const { lang, t } = useLocale()
  const count = counts[lang]
  const [draft, setDraft] = useState(emptyDraft)
  const [storageIssue, setStorageIssue] = useState(false)
  useEffect(() => {
    try { setDraft(parseDraft(localStorage.getItem(storageKey))) }
    catch { setStorageIssue(true) }
  }, [])
  function change(patch: Partial<BuildDraft>): void {
    const next = { ...draft, ...patch }
    setDraft(next)
    try { localStorage.setItem(storageKey, JSON.stringify(next)); setStorageIssue(false) }
    catch { setStorageIssue(true) }
  }
  const validStats = draft.targets.every(value => value === '' || validNumber(value, 99))
  const hasTarget = draft.targets.some(Boolean)
  const ranking = validStats && hasTarget ? compareOrigins(draft.targets.map(Number), draft.includeExtra) : []
  const best = ranking.filter(row => row.level === ranking[0].level)
  const validRange = validNumber(draft.from, 713) && validNumber(draft.to, 713) && Number(draft.to) >= Number(draft.from)
  return <>
    <div className='page-title'><div><h1>{t('빌드 계산기', 'Build Planner')}</h1><p>{t('원하는 능력치에 가장 낮은 레벨로 도달하는 태생과 필요한 룬을 비교하세요.', 'Compare starting classes to see which reaches your target attributes at the lowest level and how many runes it takes.')}</p></div></div>
    {storageIssue && <p className='callout' role='status'>{t('현재 입력을 이 기기에 저장할 수 없습니다.', 'Your changes could not be saved on this device.')}</p>}
    <Panel title={t('목표 능력치', 'Target Attributes')} extra={<button className='action small' disabled={!hasTarget} onClick={() => change({ targets: [...emptyDraft.targets] })}><TbEraser />{t('목표 지우기', 'Clear targets')}</button>}>
      <p className={styles.muted}>{t('필요한 능력치만 입력하세요. 빈칸에는 각 태생의 기본 수치를 사용합니다.', 'Enter only the attributes you need. Leave a field blank to use each class’s starting value.')}</p>
      <div id='target-attributes' className={styles.stats}>{attributes.map((attr, i) => <label className='field' key={attr}>{t(attr, englishAttributes[i])}
        <input type='number' min={1} max={99} step={1} placeholder={t('기본', 'Base')} value={draft.targets[i]}
          aria-invalid={!!draft.targets[i] && !validNumber(draft.targets[i], 99)}
          onChange={event => change({ targets: draft.targets.map((v, j) => j === i ? event.target.value : v) })} />
      </label>)}</div>
      <label className='check'><input type='checkbox' checked={draft.includeExtra}
        onChange={e => change({ includeExtra: e.target.checked })} />{t('빛바랜 자 팩 태생 포함 (중장기사 · 이데스의 기사)', 'Include Tarnished Pack classes (Heavy Knight and Idus Knight)')}</label>
      {!validStats && <p className='callout' role='alert'>{t('능력치는 1~99 사이의 정수로 입력하세요.', 'Enter a whole number from 1 to 99 for each attribute.')}</p>}
    </Panel>
    {ranking.length > 0 && <Panel title={t('태생별 비교', 'Class Comparison')} extra={<span className='count'>{ranking.length}{t('개 태생', ' classes')}</span>}>
      <div className={styles.result} role='status'><strong>{t('최저 레벨', 'Lowest level')} {best[0].level}</strong>
        <span>{best.map(row => t(row.origin.name, row.origin.english)).join(' · ')}</span></div>
      <table className={styles.table}><thead><tr><th>{t('태생', 'Class')}</th><th>{t('시작 레벨', 'Starting level')}</th><th>{t('도달 레벨', 'Final level')}</th><th>{t('시작부터 필요한 룬', 'Runes from starting level')}</th></tr></thead>
        <tbody>{ranking.map(row => <tr key={row.origin.name}><th scope='row'>{t(row.origin.name, row.origin.english)}</th>
          <td>{row.origin.level}</td><td>{row.level}</td><td>{count.format(row.runes)}</td></tr>)}</tbody></table>
    </Panel>}
    <Panel title={t('레벨 구간 룬 계산', 'Rune Calculator')}>
      <div id='rune-calculator' className={styles.range}>
        <label className='field'>{t('현재 레벨', 'Current level')}<input type='number' min={1} max={713} value={draft.from} onChange={e => change({ from: e.target.value })} /></label>
        <label className='field'>{t('목표 레벨', 'Target level')}<input type='number' min={1} max={713} value={draft.to} onChange={e => change({ to: e.target.value })} /></label>
        {validRange && <output className={styles.runes}><TbCoins />{count.format(runesBetween(Number(draft.from), Number(draft.to)))} {t('룬', 'runes')}</output>}
      </div>
      {!validRange && <p className='callout' role='alert'>{t('1~713 사이의 정수를 입력하세요. 목표 레벨은 현재 레벨 이상이어야 합니다.', 'Enter whole numbers from 1 to 713. The target level must be at least the current level.')}</p>}
    </Panel>
  </>
}
