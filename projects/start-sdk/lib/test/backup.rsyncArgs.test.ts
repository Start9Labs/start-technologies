import { execFileSync } from 'child_process'
import { existsSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'fs'
import { tmpdir } from 'os'
import { join } from 'path'
import { DEFAULT_OPTIONS, rsyncArgs } from '../backup/Backups'

describe('rsyncArgs exclusions', () => {
  let root: string
  let src: string
  let dst: string

  beforeEach(() => {
    root = mkdtempSync(join(tmpdir(), 'rsync-args-'))
    src = join(root, 'src') + '/'
    dst = join(root, 'dst') + '/'
    for (const dir of [src, dst]) {
      mkdirSync(join(dir, 'cache'), { recursive: true })
      writeFileSync(join(dir, 'cache', 'thumb'), 'x')
    }
    writeFileSync(join(src, 'data'), 'x')
  })

  afterEach(() => rmSync(root, { recursive: true, force: true }))

  const sync = (options: Partial<typeof DEFAULT_OPTIONS>) =>
    execFileSync(
      'rsync',
      rsyncArgs(src, dst, { ...DEFAULT_OPTIONS, ...options }),
    )

  it('keeps an excluded path already in the target by default', () => {
    sync({ exclude: ['/cache/'] })
    expect(existsSync(join(dst, 'data'))).toBe(true)
    expect(existsSync(join(dst, 'cache'))).toBe(true)
  })

  it('removes an excluded path from the target with deleteExcluded', () => {
    sync({ exclude: ['/cache/'], deleteExcluded: true })
    expect(existsSync(join(dst, 'data'))).toBe(true)
    expect(existsSync(join(dst, 'cache'))).toBe(false)
  })
})
