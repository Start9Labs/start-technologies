import { Effects } from '@start9labs/start-core/types'
import { asError } from '../../util'
import { HealthCheckResult } from './HealthCheckResult'
import { timeoutPromise } from './index'
import 'isomorphic-fetch'

/**
 * Check whether a URL is reachable by performing an HTTP fetch.
 *
 * Succeeds on any HTTP response (regardless of status code). Fails if the
 * request errors out or exceeds the timeout.
 *
 * @param effects - The effects context
 * @param url - The URL to fetch
 * @param options.timeout - Maximum time (ms) to wait for a response (defaults to 1000)
 * @param options.successMessage - Message shown in the UI on success
 * @param options.errorMessage - Message shown in the UI on failure
 */
export const checkWebUrl = async (
  effects: Effects,
  url: string,
  {
    timeout = 1000,
    successMessage = `Reached ${url}`,
    errorMessage = `Error while fetching URL: ${url}`,
  } = {},
): Promise<HealthCheckResult> => {
  const deadline = timeoutPromise(timeout)
  return Promise.race([fetch(url), deadline])
    .finally(deadline.cancel)
    .then(
      x =>
        ({
          result: 'success',
          message: successMessage,
        }) as const,
    )
    .catch(e => {
      console.warn(`Error while fetching URL: ${url}`)
      console.error(JSON.stringify(e))
      console.error(asError(e))
      return { result: 'failure' as const, message: errorMessage }
    })
}
