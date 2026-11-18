import { createContext, useCallback, useContext, useEffect, useState } from 'react'
import type { ReactElement, ReactNode } from 'react'

export type Language = 'ko' | 'en'
export type Translate = (korean: string, english: string) => string
const korean: Translate = ko => ko
const english: Translate = (_ko, en) => en
const storageKey = 'elden-ring-mod-assist.theme.language'

interface LocaleState {
  lang: Language
  setLang: (language: Language) => void
  t: Translate
}

const LocaleContext = createContext<LocaleState | null>(null)

export function LocaleProvider({ children }: { children: ReactNode }): ReactElement {
  const [lang, update] = useState<Language>('ko')
  useEffect(() => {
    try {
      if (localStorage.getItem(storageKey) === 'en') update('en')
    } catch { /* Use Korean when storage is unavailable. */ }
  }, [])
  useEffect(() => { document.documentElement.lang = lang }, [lang])
  const setLang = useCallback((next: Language) => {
    update(next)
    try { localStorage.setItem(storageKey, next) } catch { /* The change applies to this session. */ }
  }, [])
  return <LocaleContext.Provider value={{ lang, setLang, t: lang === 'en' ? english : korean }}>{children}</LocaleContext.Provider>
}

export function useLocale(): LocaleState {
  const locale = useContext(LocaleContext)
  if (!locale) throw new Error('LocaleProvider is missing')
  return locale
}
