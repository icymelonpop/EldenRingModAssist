import { createHash } from 'node:crypto'
import { execFileSync } from 'node:child_process'
import { readFile, writeFile, mkdir, mkdtemp, copyFile, rename, stat, access, rm } from 'node:fs/promises'
import { basename, dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

// Pin the exact published ZIP and keep its two licenses with the portable runtime.
const version = '0.13.0'
const sha256 = 'a8b693c574106f20532ab1e8b58b2452f37370679b938a884fe75f87f9b68c2b'
const url = 'https://github.com/garyttierney/me3/releases/download/v' + version + '/me3-windows-amd64.zip'
const target = resolve(dirname(fileURLToPath(import.meta.url)), '../src-tauri/target/release')
const dest = join(target, 'me3-' + version)
const required = ['bin/me3.exe', 'bin/me3-launcher.exe', 'bin/me3_mod_host.dll', 'LICENSE-APACHE', 'LICENSE-MIT']
const digest = data => createHash('sha256').update(data).digest('hex')
const readArchive = async () => {
  if (process.argv[2]) return readFile(resolve(process.argv[2]))
  const response = await fetch(url)
  if (!response.ok) throw new Error('me3 다운로드 실패 (' + response.status + ')')
  return Buffer.from(await response.arrayBuffer())
}
await mkdir(target, { recursive: true })
const work = await mkdtemp(join(target, '.prepare-me3-'))
try {
  const archive = await readArchive()
  if (digest(archive) !== sha256) throw new Error('공식 ZIP의 SHA-256과 일치하지 않습니다.')
  const archivePath = join(work, 'me3.zip')
  await writeFile(archivePath, archive, { flag: 'wx' })
  const unpacked = join(work, 'unpacked')
  await mkdir(unpacked)
  execFileSync('tar.exe', ['-xf', archivePath, '-C', unpacked], { windowsHide: true })
  const bundle = join(work, 'portable')
  await mkdir(join(bundle, 'bin'), { recursive: true })
  let alreadyExists = false
  try { await access(dest); alreadyExists = true } catch (e) { if (e.code !== 'ENOENT') throw e }
  for (const name of required) {
    const src = join(unpacked, name)
    if (!(await stat(src)).isFile()) throw new Error('배포본에서 파일을 찾을 수 없습니다. ' + name)
    if (alreadyExists) {
      if (digest(await readFile(src)) !== digest(await readFile(join(dest, name)))) {
        throw new Error('기존 로더 파일이 다릅니다. 확인 후 다시 빌드하세요. ' + name)
      }
    } else await copyFile(src, join(bundle, name))
  }
  if (!alreadyExists) await rename(bundle, dest)
  console.log('Verified official portable runtime: ' + basename(dest))
} finally {
  // work is an exclusive mkdtemp directory under Cargo's release directory.
  await rm(work, { recursive: true, force: true })
}
