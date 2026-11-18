import { useLocale } from '@/locale'
import type { ReactElement } from 'react'
import { TbChecklist, TbFingerprint, TbShieldCheck, TbShieldExclamation } from 'react-icons/tb'
import type { Report } from '@/native'
import { Panel } from './Controls'

interface Props {
  source?: 'steam' | 'baseline'
  report: Report | null
  enabled: boolean
  verify: () => void
}

export default function Integrity({ report, enabled, verify, source = 'baseline' }: Props): ReactElement {
  const { t } = useLocale()
  const good = Boolean(report?.matches && !report.stale)
  return (
    <Panel title={source === 'steam' ? t('Steam 공식 원본 대조', 'Compare with the Steam Release') : t('바닐라 무결성 검사', 'Verify Vanilla Files')} extra={<button type='button' className='action small' onClick={verify} disabled={!enabled}><TbChecklist />{t('전체 검사', 'Verify All Files')}</button>}>
      <div className={`integrity ${report ? good ? 'good' : 'warn' : ''}`}>
        {report ? good ? <TbShieldCheck /> : <TbShieldExclamation /> : <TbFingerprint />}
        <div><strong>{report ? good ? t('기준 파일과 일치', 'Files Match the Baseline') : t('확인이 필요한 차이 발견', 'Differences Found') : source === 'steam' ? t('공식 배포 파일 대조', 'Compare Official Game Files') : t('바닐라 파일 대조', 'Compare Vanilla Files')}</strong>
          <p>{report ? t(`${report.checked.toLocaleString('ko-KR')}개 파일 검사 · 추가 ${report.added.length} · 변경 ${report.changed.length} · 누락 ${report.missing.length}`, `Files checked: ${report.checked.toLocaleString('en-US')} · Added: ${report.added.length} · Changed: ${report.changed.length} · Missing: ${report.missing.length}`) :
            source === 'steam' ? t('Steam 배포 목록으로 누락·변경 파일과 추가된 모드 파일을 찾습니다.', 'Check Steam’s file list for missing or changed files and added mods.') : t('등록한 원본과 파일 목록 및 내용을 비교합니다.', 'Compare file names and contents with your registered baseline.')}</p>
        </div>
      </div>
      {report?.stale && <p className='callout'>{t('Steam 빌드가 바뀌었습니다. 설치 설정에서 새 원본을 등록하세요.', 'The Steam build has changed. Register a new baseline in Settings.')}</p>}
      {report && ([[t('추가', 'Added'), report.added], [t('변경', 'Changed'), report.changed], [t('누락', 'Missing'), report.missing]] as const).map(([label, files]) => files.length > 0 &&
        <details className='differences' key={label}><summary>{label}{t('된 파일 ', ' files: ')}{files.length.toLocaleString('ko-KR')}{t('개', '')}</summary>
          <ul>{files.slice(0, 200).map(file => <li key={file}>{file}</li>)}</ul>
          {files.length > 200 && <small>{t('처음 200개 경로를 표시합니다.', 'Showing the first 200 paths.')}</small>}
        </details>
      )}
    </Panel>
  )
}
