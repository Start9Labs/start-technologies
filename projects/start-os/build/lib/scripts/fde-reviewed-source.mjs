import { execFile } from 'node:child_process'
import { createHash } from 'node:crypto'
import { promisify } from 'node:util'

const exec = promisify(execFile)

/** Executes an admitted provider in the stable public source namespace, excluding inherited overrides. */
export function runSource(file, args) {
  return exec(file, args, {
    env: { PATH: '/usr/sbin:/usr/bin:/sbin:/bin', LC_ALL: 'C' },
    maxBuffer: 1024 * 1024,
  })
}

export function requireReviewedContext(bytes, expectedDigest, name) {
  if (
    !expectedDigest ||
    createHash('sha256').update(bytes).digest('hex') !== expectedDigest
  )
    throw Error(`Unsupported generated context: ${name}`)
}

export function replaceOnce(text, before, after) {
  if (text.split(before).length !== 2)
    throw Error('Unsupported generated context')
  return text.replace(before, after)
}
