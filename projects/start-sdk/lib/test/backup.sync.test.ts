import { execFileSync } from 'child_process'
import { existsSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'fs'
import { readdir } from 'fs/promises'
import { tmpdir } from 'os'
import { join } from 'path'
import {
  DEFAULT_OPTIONS,
  pruneBackupVolumes,
  rsyncArgs,
} from '../backup/Backups'

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

  it('removes an excluded path already in the target', () => {
    sync({ exclude: ['/cache/'] })
    expect(existsSync(join(dst, 'data'))).toBe(true)
    expect(existsSync(join(dst, 'cache'))).toBe(false)
  })

  it('leaves the target untouched without delete', () => {
    sync({ exclude: ['/cache/'], delete: false })
    expect(existsSync(join(dst, 'data'))).toBe(true)
    expect(existsSync(join(dst, 'cache'))).toBe(true)
  })
})

describe('pruneBackupVolumes', () => {
  let volumes: string

  beforeEach(() => {
    volumes = join(mkdtempSync(join(tmpdir(), 'backup-volumes-')), 'volumes')
    for (const v of ['main', 'db', 'removed']) {
      mkdirSync(join(volumes, v), { recursive: true })
      writeFileSync(join(volumes, v, 'file'), 'x')
    }
  })

  afterEach(() => rmSync(join(volumes, '..'), { recursive: true, force: true }))

  it('removes volumes no backup path lies within', async () => {
    await pruneBackupVolumes(volumes, [
      `${volumes}/main/`,
      `${volumes}/db/pgdata/`,
      `${join(volumes, '..')}/other/`,
    ])
    expect((await readdir(volumes)).sort()).toEqual(['db', 'main'])
  })

  it('keeps every volume when a backup path is the volumes directory', async () => {
    await pruneBackupVolumes(volumes, [`${volumes}/`])
    expect((await readdir(volumes)).sort()).toEqual(['db', 'main', 'removed'])
  })

  it('tolerates a backup with no volumes directory', async () => {
    await expect(
      pruneBackupVolumes(join(volumes, 'missing'), []),
    ).resolves.toBeUndefined()
  })
})
