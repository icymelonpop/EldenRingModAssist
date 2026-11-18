export const attributes = ['생명력', '정신력', '지구력', '근력', '기량', '지력', '신앙', '신비'] as const

export const englishAttributes = ['Vigor', 'Mind', 'Endurance', 'Strength', 'Dexterity', 'Intelligence', 'Faith', 'Arcane'] as const

interface Origin {
  name: string
  english: string
  level: number
  stats: number[]
  extra?: boolean
}

export const origins: Origin[] = [
  { name: '방랑기사', english: 'Vagabond', level: 9, stats: [15, 10, 11, 14, 13, 9, 9, 7] },
  { name: '검사', english: 'Warrior', level: 8, stats: [11, 12, 11, 10, 16, 10, 8, 9] },
  { name: '용사', english: 'Hero', level: 7, stats: [14, 9, 12, 16, 9, 7, 8, 11] },
  { name: '도적', english: 'Bandit', level: 5, stats: [10, 11, 10, 9, 13, 9, 8, 14] },
  { name: '점성술사', english: 'Astrologer', level: 6, stats: [9, 15, 9, 8, 12, 16, 7, 9] },
  { name: '예언자', english: 'Prophet', level: 7, stats: [10, 14, 8, 11, 10, 7, 16, 10] },
  { name: '무사', english: 'Samurai', level: 9, stats: [12, 11, 13, 12, 15, 9, 8, 8] },
  { name: '죄수', english: 'Prisoner', level: 9, stats: [11, 12, 11, 11, 14, 14, 6, 9] },
  { name: '밀사', english: 'Confessor', level: 10, stats: [10, 13, 10, 12, 12, 9, 14, 9] },
  { name: '빈털터리', english: 'Wretch', level: 1, stats: [10, 10, 10, 10, 10, 10, 10, 10] },
  { name: '중장기사', english: 'Heavy Knight', level: 10, stats: [14, 8, 17, 15, 11, 7, 8, 9], extra: true },
  { name: '이데스의 기사', english: 'Idus Knight', level: 7, stats: [10, 12, 11, 13, 15, 8, 11, 6], extra: true },
]

// Use integer hundredths for the growth coefficient to avoid rounding down
// an exact integer cost by one rune at some of the higher levels.
const totalRunes = [0, 0]
for (let level = 1; level < 713; level++) {
  const coefficient = 10 + 2 * Math.max(0, level - 11)
  const next = 1 + Math.floor(coefficient * (level + 81) ** 2 / 100)
  totalRunes.push(totalRunes[level] + next)
}

export function runesBetween(from: number, to: number): number {
  return totalRunes[to] - totalRunes[from]
}

interface Comparison {
  origin: Origin
  level: number
  points: number
  runes: number
}
export function compareOrigins(targets: number[], includeExtra: boolean): Comparison[] {
  return origins.filter(origin => includeExtra || !origin.extra).map(origin => {
    const points = origin.stats.reduce((sum, base, i) => sum + Math.max(0, targets[i] - base), 0)
    const level = origin.level + points
    return { origin, level, points, runes: runesBetween(origin.level, level) }
  }).sort((a, b) => a.level - b.level || a.runes - b.runes)
}

export interface BuildDraft {
  targets: string[]
  includeExtra: boolean
  from: string
  to: string
}
export const emptyDraft: BuildDraft = {
  targets: attributes.map(() => ''), includeExtra: false, from: '1', to: '150',
}
export const validNumber = (value: string, limit: number): boolean =>
  /^\d+$/.test(value) && Number(value) >= 1 && Number(value) <= limit

export function parseDraft(raw: string | null): BuildDraft {
  if (!raw) return emptyDraft
  try {
    const value = JSON.parse(raw)
    if (value && Array.isArray(value.targets) && value.targets.length === attributes.length &&
        value.targets.every((x: unknown) => typeof x === 'string') && typeof value.from === 'string' &&
        typeof value.to === 'string' && typeof value.includeExtra === 'boolean') return value
  } catch { /* An older or unreadable draft can be replaced by a new one. */ }
  return emptyDraft
}
