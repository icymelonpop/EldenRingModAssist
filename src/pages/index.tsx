import type { ReactElement } from 'react'
import Head from 'next/head'
import App from '@/App'
import RingShell from '@/components/RingShell'
import { LocaleProvider, useLocale } from '@/locale'

function LocalizedApp(): ReactElement {
  const { t } = useLocale()
  return (
    <>
      <Head>
        <title>{t('엘든 링 모드 어시스트 · 황금나무', 'Elden Ring Mod Assist · Erdtree')}</title>
        <meta name='description' content={t('엘든 링 바닐라 복원, 모드 프로필 전환 및 세이브 백업', 'Restore vanilla files, switch mod profiles and back up your Elden Ring saves.')} />
      </Head>
      <RingShell><App /></RingShell>
    </>
  )
}

export default function Index(): ReactElement {
  return <LocaleProvider><LocalizedApp /></LocaleProvider>
}
