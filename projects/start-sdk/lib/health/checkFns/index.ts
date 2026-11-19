import { runHealthScript } from './runHealthScript'
export { checkPortListening } from './checkPortListening'
export { HealthCheckResult } from './HealthCheckResult'
export { checkWebUrl } from './checkWebUrl'

/**
 * Create a promise that rejects after the specified timeout.
 * Useful for racing against long-running health checks.
 *
 * @param ms - Timeout duration in milliseconds
 * @param options.message - Custom error message (defaults to "Timed out")
 * @returns A rejecting deadline; `cancel()` clears an unused timer.
 */
export function timeoutPromise(ms: number, { message = 'Timed out' } = {}) {
  let timer!: ReturnType<typeof setTimeout>
  return Object.assign(
    new Promise<never>((_, reject) => {
      timer = setTimeout(() => reject(new Error(message)), ms)
    }),
    { cancel: () => clearTimeout(timer) },
  )
}
export { runHealthScript }
