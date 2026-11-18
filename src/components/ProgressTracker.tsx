import { useCallback, useEffect, useRef, useState } from 'react'
import type { ReactElement } from 'react'
import { TbChecklist, TbMap, TbPencil, TbPlus, TbTrash } from 'react-icons/tb'
import { ConfirmDialog, Panel } from './Controls'
import type { Question } from './Controls'
import { useLocale } from '@/locale'
import { catalog, categories, categoryLabel, entryText } from './trackerCatalog'
import type { Category, Entry } from './trackerCatalog'
import styles from '@/styles/tracker.module.css'

interface Goal { id: string; name: string }
interface Journey { id: string; name: string; userNamed?: boolean; checked: string[]; goals: Goal[] }
interface Notebook { version: 1; active: string; journeys: Journey[] }
const storageKey = 'elden-ring-mod-assist.journey-notebook.v1'
const emptyBook = (): Notebook => ({
  version: 1, active: 'first', journeys: [{ id: 'first', name: '첫 번째 여정', checked: [], goals: [] }],
})

function journeyName(journey: Journey, lang: 'ko' | 'en'): string {
  // Older notebooks identify the built-in starter by its original id and name.
  if (lang === 'en' && journey.id === 'first' && !journey.userNamed && journey.name === '첫 번째 여정') return 'First Journey'
  return journey.name
}

const notebookErrors: Record<string, string> = {
  '저장한 여정의 형식을 읽을 수 없습니다.': 'The saved journey format could not be read.',
  '이 여정이 다른 창에서 삭제되었습니다.': 'This journey was deleted in another window.',
  '같은 이름의 여정이 있습니다.': 'A journey already has this name.',
  '마지막 여정은 유지해야 합니다.': 'Keep at least one journey in the journal.',
}
type NotebookIssue = { kind: 'read' | 'storage' } | { kind: 'write'; detail: string }

function readBook(): Notebook {
  const raw = localStorage.getItem(storageKey)
  if (raw === null) return emptyBook()
  const book: Notebook = JSON.parse(raw)
  if (book?.version !== 1 || !Array.isArray(book.journeys) || !book.journeys.length ||
    !book.journeys.every(j => j && typeof j.id === 'string' && typeof j.name === 'string' &&
      Array.isArray(j.checked) && j.checked.every(id => typeof id === 'string') &&
      Array.isArray(j.goals) && j.goals.every(g => g && typeof g.id === 'string' && typeof g.name === 'string')) ||
    !book.journeys.some(j => j.id === book.active)) {
    throw new Error('저장한 여정의 형식을 읽을 수 없습니다.')
  }
  return book
}

function changeJourney(book: Notebook, id: string, change: (journey: Journey) => Journey): Notebook {
  if (!book.journeys.some(journey => journey.id === id)) throw new Error('이 여정이 다른 창에서 삭제되었습니다.')
  return { ...book, journeys: book.journeys.map(journey => journey.id === id ? change(journey) : journey) }
}

function entries(category: Category, journey: Journey): Entry[] {
  if (category !== 'custom') return catalog[category]
  return journey.goals.map(goal => ({ ...goal, place: '', quantity: 1 }))
}

interface Props { openMap: () => void }
export default function ProgressTracker({ openMap }: Props): ReactElement {
  const { lang, t } = useLocale()
  const [book, setBook] = useState<Notebook | null>(null)
  const [category, setCategory] = useState<Category>('base')
  const [search, setSearch] = useState('')
  const [onlyRemaining, setOnlyRemaining] = useState(false)
  const [question, setQuestion] = useState<Question | null>(null)
  const [error, setError] = useState<NotebookIssue | null>(null)
  const pending = useRef(new Set<(current: Notebook) => Notebook>())
  const refresh = useCallback((): void => {
    let next = readBook()
    for (const change of pending.current) {
      try { next = change(next) }
      catch { /* A concurrent edit can invalidate this action. Its commit reports the conflict. */ }
    }
    setBook(next)
  }, [])

  useEffect(() => {
    function load(): void {
      try { refresh(); setError(null) }
      catch { setBook(null); setError({ kind: 'read' }) }
    }
    function onStorage(event: StorageEvent): void {
      if (event.key === storageKey || event.key === null) load()
    }
    load()
    window.addEventListener('storage', onStorage)
    return () => window.removeEventListener('storage', onStorage)
  }, [refresh])

  function update(change: (current: Notebook) => Notebook): void {
    // Read-modify-write under one WebView lock so another open app window can
    // update a different journey without losing its changes.
    const save = (): void => { localStorage.setItem(storageKey, JSON.stringify(change(readBook()))) }
    async function commit(): Promise<void> {
      try {
        pending.current.add(change)
        refresh()
        if (navigator.locks) await navigator.locks.request(storageKey, save)
        else save()
        setError(null)
      } catch (problem) {
        setError(problem instanceof Error ? { kind: 'write', detail: problem.message } : { kind: 'storage' })
      } finally {
        pending.current.delete(change)
        try { refresh() } catch { setBook(null) }
      }
    }
    void commit()
  }

  function nameJourney(current?: Journey): void {
    setQuestion({
      title: current ? t('여정 이름 변경', 'Rename journey') : t('새 여정', 'New journey'),
      message: t('캐릭터나 회차를 구분할 수 있는 이름을 입력하세요.', 'Enter a name for this character or playthrough.'),
      button: current ? t('저장', 'Save') : t('여정 만들기', 'Create journey'),
      input: { label: t('여정 이름', 'Journey name'), initial: current ? journeyName(current, lang) : '' },
      action: name => {
        const id = crypto.randomUUID()
        update(latest => {
          if (latest.journeys.some(j => j.id !== current?.id && (j.name === name || journeyName(j, lang) === name))) throw new Error('같은 이름의 여정이 있습니다.')
          if (current) return changeJourney(latest, current.id, j => ({ ...j, name, userNamed: true }))
          return { ...latest, active: id, journeys: [...latest.journeys, { id, name, checked: [], goals: [] }] }
        })
      },
    })
  }

  function removeJourney(journey: Journey): void {
    setQuestion({
      title: t('여정 삭제', 'Delete journey'), message: t('“' + journey.name + '”의 완료 기록과 직접 추가한 목표를 삭제합니다.', 'Delete the progress and personal goals for “' + journeyName(journey, lang) + '”?'), button: t('삭제', 'Delete'),
      action: () => update(latest => {
        const journeys = latest.journeys.filter(j => j.id !== journey.id)
        if (!journeys.length) throw new Error('마지막 여정은 유지해야 합니다.')
        return { ...latest, journeys, active: latest.active === journey.id ? journeys[0].id : latest.active }
      }),
    })
  }

  function addGoal(journey: Journey): void {
    setQuestion({
      title: t('나의 목표 추가', 'Add a personal goal'), message: t('이번 여정에서 찾아갈 NPC, 얻고 싶은 장비, 다음 할 일 등을 기록하세요.', 'Add an NPC to visit, equipment to find or another goal for this journey.'),
      button: t('추가', 'Add'), input: { label: t('목표', 'Goal'), initial: '' },
      action: name => {
        const id = 'goal-' + crypto.randomUUID()
        setSearch(''); setOnlyRemaining(false)
        update(latest => changeJourney(latest, journey.id, j => ({ ...j, goals: [...j.goals, { id, name }] })))
      },
    })
  }

  function removeGoal(journeyId: string, entry: Entry): void {
    setQuestion({
      title: t('목표 삭제', 'Delete goal'), message: t('“' + entry.name + '”을 이 여정의 목표에서 삭제합니다.', 'Remove “' + entry.name + '” from this journey’s goals?'), button: t('삭제', 'Delete'),
      action: () => update(latest => changeJourney(latest, journeyId, j => ({
        ...j, goals: j.goals.filter(goal => goal.id !== entry.id), checked: j.checked.filter(id => id !== entry.id),
      }))),
    })
  }

  function mark(journeyId: string, entry: Entry, checked: boolean): void {
    update(latest => changeJourney(latest, journeyId, j => {
      if (entry.id.startsWith('goal-') && !j.goals.some(goal => goal.id === entry.id)) return j
      const ids = new Set(j.checked)
      if (checked) ids.add(entry.id)
      else ids.delete(entry.id)
      return { ...j, checked: Array.from(ids) }
    }))
  }

  const journey = book?.journeys.find(item => item.id === book.active)
  const checked = new Set(journey?.checked)
  const list = journey ? entries(category, journey) : []
  const query = search.trim().replace(/\s/g, '').toLocaleLowerCase()
  const visible = list.filter(e => (!onlyRemaining || !checked.has(e.id)) &&
    [e.name, e.place, e.reward, e.english?.name, e.english?.place, e.english?.reward].join(' ').replace(/\s/g, '').toLocaleLowerCase().includes(query))
  const pickup = category === 'fragments' || category === 'ashes'
  return <>
    <div className='page-title'><div><h1>{t('모험 수첩', 'Adventure Journal')}</h1><p>{t('캐릭터·회차별로 보스 처치와 수집 진행을 직접 기록하세요.', 'Track defeated bosses and collectibles for each character or playthrough.')}</p></div>
      <button id='journal-map' type='button' className='action' onClick={openMap}><TbMap />{t('한글 지도 열기', 'Open Korean map')}</button>
    </div>
    {error && <p className='callout' role='alert'>{error.kind === 'read'
      ? t('이 기기에 저장된 모험 수첩을 읽을 수 없습니다. 앱의 저장 공간을 확인해 주세요.', 'The journal could not be read. Check the app’s storage on this device.')
      : error.kind === 'write' ? t('수첩 저장 실패. ' + error.detail, 'Could not save the journal. ' + (notebookErrors[error.detail] ?? error.detail))
        : t('수첩 저장 공간을 확인해 주세요.', 'Check the storage available to your journal.')}</p>}
    {book && journey && <>
      <section className={styles.journey}>
        <label className='field'>{t('기록할 여정', 'Active journey')}
          <select value={journey.id} onChange={e => {
            const active = e.target.value
            update(latest => {
              if (!latest.journeys.some(j => j.id === active)) throw new Error('이 여정이 다른 창에서 삭제되었습니다.')
              return { ...latest, active }
            })
          }}>{book.journeys.map(j => <option key={j.id} value={j.id}>{journeyName(j, lang)}</option>)}</select>
        </label>
        <div className={styles.tools}>
          <button id='journal-new' type='button' className='action small' onClick={() => nameJourney()}><TbPlus />{t('새 여정', 'New journey')}</button>
          <button type='button' className='action quiet small' onClick={() => nameJourney(journey)}><TbPencil />{t('이름 변경', 'Rename')}</button>
          {book.journeys.length > 1 && <button type='button' className='icon-btn' aria-label={t('현재 여정 삭제', 'Delete current journey')} onClick={() => removeJourney(journey)}><TbTrash /></button>}
        </div>
      </section>
      <div className={styles.categories} role='group' aria-label={t('목록 종류', 'Journal categories')}>
        {categories.map(c => {
          const items = entries(c.id, journey)
          const total = items.reduce((n, e) => n + e.quantity, 0)
          const done = items.reduce((n, e) => n + (checked.has(e.id) ? e.quantity : 0), 0)
          return <button key={c.id} type='button' data-journal-tab={c.id} aria-pressed={category === c.id} onClick={() => { setCategory(c.id); setSearch('') }}>
            <span>{categoryLabel(c.id, lang)}</span><span className={styles.progress}>{done} / {total}</span>
          </button>
        })}
      </div>
      <Panel title={categoryLabel(category, lang)}
        extra={category === 'custom' ? <button type='button' className='action small' onClick={() => addGoal(journey)}><TbPlus />{t('목표 추가', 'Add goal')}</button>
          : <span className='count'>{list.length}{t(pickup ? '곳' : category === 'preparation' ? '개' : '종', pickup ? ' locations' : category === 'preparation' ? ' tasks' : ' bosses')}</span>}>
        <div className={styles.filters}>
          <input type='search' aria-label={t('수첩에서 검색', 'Search journal')} placeholder={category === 'custom' || category === 'preparation' ? t('목표 검색', 'Search goals') : pickup ? t('지역 또는 획득 지점 검색', 'Search areas or pickup locations') : t('보스, 지역 또는 추억 검색', 'Search bosses, locations or Remembrances')}
            value={search} onChange={e => setSearch(e.target.value)} />
          <label className='check'><input type='checkbox' checked={onlyRemaining} onChange={e => setOnlyRemaining(e.target.checked)} />{t('미완료만', 'Incomplete only')}</label>
        </div>
        <div className={styles.rows}>{visible.map(entry => {
          const text = entryText(entry, lang)
          return <div key={entry.id} className={styles.row}>
          <label><input type='checkbox' checked={checked.has(entry.id)} onChange={e => mark(journey.id, entry, e.target.checked)} />
            <span><strong>{text.name}</strong>{text.place && <small>{text.place}{text.reward && ' · ' + text.reward}</small>}</span>
            {pickup && <span className={styles.quantity}>{t(entry.quantity + '개', '×' + entry.quantity)}</span>}
          </label>
          {category === 'custom' && <button type='button' className='icon-btn' aria-label={t(entry.name + ' 삭제', 'Delete ' + entry.name)} onClick={() => removeGoal(journey.id, entry)}><TbTrash /></button>}
        </div>
        })}</div>
        {!visible.length && <div className='empty'><TbChecklist /><p>{!list.length ? t('목표 추가를 눌러 이번 여정에서 할 일을 기록하세요.', 'Select Add goal to record something to do on this journey.') :
          search ? t('검색 조건에 맞는 항목이 없습니다.', 'No entries match your search.') : t('이 목록의 모든 항목을 완료했습니다.', 'You have completed every entry on this list.')}</p></div>}
      </Panel>
    </>}
    {question && <ConfirmDialog question={question} close={() => setQuestion(null)} />}
  </>
}
