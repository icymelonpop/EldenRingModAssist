import englishCatalog from './nativeEnglish.json'

type Language = 'ko' | 'en'

const messages: Readonly<Record<string, string>> = englishCatalog
const placeholder = /\{(\w*)\}/g
const escapeRegex = (value: string): string => value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')

// Match only complete messages emitted by the native application. Captures such as
// filenames, game paths and profile IDs are put back without translating their text.
const patterns = Object.entries(messages).filter(([ko]) => ko.includes('{')).map(([ko, en]) => {
  const tokens = ko.split(placeholder)
  const slots = tokens.filter((_, i) => i % 2 === 1)
  const pattern = tokens.map((part, i) => i % 2 ? '([\\s\\S]*?)' : escapeRegex(part)).join('')
  return { match: new RegExp('^' + pattern + '$'), slots, en }
})

// std::io::Error uses the Windows display language. Keep its numeric code when
// giving English wording for the common filesystem errors.
const systemErrors: Readonly<Record<string, string>> = {
  '2': 'The file was not found.', '3': 'The path was not found.', '5': 'Access was denied.',
  '21': 'The device is not ready.', '32': 'Another process is using the file.',
  '33': 'Another process has locked the file.', '80': 'The file already exists.',
  '87': 'The parameter is invalid.', '112': 'There is not enough space on the disk.',
  '123': 'The path or file name is invalid.', '145': 'The directory is not empty.',
  '183': 'An item with this name already exists.', '206': 'The path or extension is too long.',
}

function englishMessage(text: string): string {
  if (Object.prototype.hasOwnProperty.call(messages, text)) return messages[text]
  if (text.startsWith('Error: ')) return 'Error: ' + englishMessage(text.slice(7))
  for (const { match, slots, en } of patterns) {
    const values = match.exec(text)
    if (!values) continue
    let i = 0
    return en.replace(placeholder, () => {
      const slot = slots[i]
      const value = values[++i]
      if (slot === 'backup_kind') return messages[value] ?? value
      return ['e', 'error', 'reason'].includes(slot) ? englishMessage(value) : value
    })
  }
  // Native filesystem errors use either separator after one or more paths.
  // Preserve the entire prefix even when a filename itself contains a separator.
  const at = Math.max(text.lastIndexOf(' — '), text.lastIndexOf(' · '))
  if (at !== -1) return text.slice(0, at + 3) + englishMessage(text.slice(at + 3))
  const newline = text.lastIndexOf('\n')
  if (newline !== -1) return text.slice(0, newline + 1) + englishMessage(text.slice(newline + 1))
  const win32 = / \(os error (-?\d+)\)$/.exec(text)
  if (win32) {
    const code = win32[1]
    const description = systemErrors[code]
    return description ? `${description} (Windows error ${code})` : `Windows error ${code}`
  }
  return text
}

export function translateNative(text: string, lang: Language): string {
  return lang === 'en' ? englishMessage(text) : text
}
