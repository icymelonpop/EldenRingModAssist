import type { ComponentProps, ReactElement } from 'react'
import { TbMoon, TbSun } from 'react-icons/tb'
import { Panel } from './Controls'
import { useLocale } from '@/locale'
import NativeSettings from './NativeSettings'
import { useRingShell } from './RingShell'
import type { GlassLevel } from './RingShell'

export default function Settings(props: ComponentProps<typeof NativeSettings>): ReactElement {
  const { glass, setGlass, themeMode, setThemeMode } = useRingShell()
  const { lang, setLang, t } = useLocale()
  return (
    <>
      <NativeSettings {...props} />
      <Panel title={t('화면 효과', 'Appearance')}>
        <div className='ring-appearance'>
          <label className='field' htmlFor='interface-language'>
            {t('표시 언어', 'Display language')} <span lang={lang === 'ko' ? 'en' : 'ko'}>· {t('Language', '언어')}</span>
          </label>
          <select id='interface-language' value={lang} onChange={event => setLang(event.currentTarget.value === 'en' ? 'en' : 'ko')}>
            <option value='ko' lang='ko'>한국어</option>
            <option value='en' lang='en'>English</option>
          </select>
        </div>
        <div className='ring-appearance'>
          <span className='field' id='theme-mode-label'>{t('화면 모드', 'Color mode')}</span>
          <div className='ring-mode-picker' role='radiogroup' aria-labelledby='theme-mode-label'>
            <label>
              <input type='radio' name='theme-mode' value='light' checked={themeMode === 'light'} onChange={() => setThemeMode('light')} />
              <span><TbSun aria-hidden />{t('화이트', 'Light')}</span>
            </label>
            <label>
              <input type='radio' name='theme-mode' value='dark' checked={themeMode === 'dark'} onChange={() => setThemeMode('dark')} />
              <span><TbMoon aria-hidden />{t('블랙', 'Dark')}</span>
            </label>
          </div>
        </div>
        <div className='ring-appearance'>
          <div>
            <label className='field' htmlFor='background-glass'>{t('배경 비침', 'Transparency')}</label>
            <p id='glass-hint'>{t('창과 패널 너머로 배경이 비치는 정도를 조절합니다.', 'Choose how much of the background shows through the window and panels.')}
              <span className='glass-system-note'>{t('시스템의 접근성 설정에 따라 현재는 불투명하게 표시합니다.', 'Your system accessibility settings currently keep the window opaque.')}</span>
            </p>
          </div>
          <select id='background-glass' value={glass} aria-describedby='glass-hint'
            onChange={event => setGlass(event.currentTarget.value as GlassLevel)}>
            <option value='normal'>{t('은은하게', 'Subtle')}</option>
            <option value='high'>{t('더 선명하게', 'More transparent')}</option>
            <option value='off'>{t('불투명', 'Opaque')}</option>
          </select>
        </div>
      </Panel>
    </>
  )
}
