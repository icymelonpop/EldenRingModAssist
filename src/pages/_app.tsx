import type { AppProps } from 'next/app'
import type { ReactElement } from 'react'

import '@/styles/globals.css'
import '@/styles/elden-theme.css'
import '@/styles/appearance.css'
import '@/styles/typography.css'
import '@/styles/caption.css'

export default function App({ Component, pageProps }: AppProps): ReactElement {
  return <Component {...pageProps} />
}
