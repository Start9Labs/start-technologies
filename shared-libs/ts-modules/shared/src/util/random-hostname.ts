import { ADJECTIVES, NOUNS } from './hostname-words'

/** Returns an adjective-noun hostname. */
export function randomHostname(): string {
  const adj = ADJECTIVES[Math.floor(Math.random() * ADJECTIVES.length)]!
  const noun = NOUNS[Math.floor(Math.random() * NOUNS.length)]!

  return `${adj}-${noun}`
}
