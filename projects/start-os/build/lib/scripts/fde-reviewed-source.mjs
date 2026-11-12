import { createHash } from 'node:crypto'

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
