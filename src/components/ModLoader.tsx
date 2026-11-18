import { useLocale } from '@/locale'
import type { ReactElement } from 'react'
import { TbDownload, TbFileSearch, TbRefresh } from 'react-icons/tb'
import type { LoaderStatus } from '@/native'
import { Panel } from './Controls'

interface Props {
  loader: LoaderStatus | null
  disabled: boolean
  select: () => void
  detect: () => void
  download: () => void
}

export default function ModLoader({ loader, disabled, select, detect, download }: Props): ReactElement {
  const { t } = useLocale()
  return <div id='mod-loader' tabIndex={-1}>
    <Panel title={t('me3 모드 로더', 'me3 Mod Loader')} extra={<span className='count'>{loader?.installed ? t('연결됨', 'Connected') : loader ? t('연결 필요', 'Connection required') : t('확인 전', 'Not checked')}</span>}>
      <p>{t('경량 프로필 실행에는 배포 폴더에 함께 제공된 me3를 사용합니다. 다른 위치에 설치한 경우 실행 파일을 선택하세요.', 'Lightweight profiles use the copy of me3 included with this app. If you installed it elsewhere, select its executable.')}</p>
      {loader?.path && <dl className='facts'><dt>{t('실행 파일', 'Executable')}</dt><dd>{loader.path}</dd></dl>}
      <div className='actions'>
        {!loader?.installed && <button className='action small' disabled={disabled} onClick={download}><TbDownload />{t('me3 공식 다운로드', 'Official me3 Download')}</button>}
        <button className='action small' disabled={disabled} onClick={detect}><TbRefresh />{t('자동 찾기', 'Auto-Detect')}</button>
        <button className='action small' disabled={disabled} onClick={select}><TbFileSearch />{t('me3.exe 선택', 'Select me3.exe')}</button>
      </div>
    </Panel>
  </div>
}
