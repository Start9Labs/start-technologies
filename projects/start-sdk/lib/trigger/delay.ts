const MAX_TIMEOUT_MS = 2 ** 31 - 1

/** Resolves after `ms`, however long; never resolves for `Infinity`. */
export async function delay(ms: number): Promise<void> {
  if (ms === Infinity) return new Promise(() => {})
  for (; ms > MAX_TIMEOUT_MS; ms -= MAX_TIMEOUT_MS)
    await new Promise(resolve => setTimeout(resolve, MAX_TIMEOUT_MS))
  await new Promise(resolve => setTimeout(resolve, ms))
}
