import { HealthCheckResult } from './HealthCheckResult'
import { timeoutPromise } from './index'
import { SubContainer } from '../../util/SubContainer'
import { SDKManifest } from '../../types'

/**
 * Run a command in a subcontainer and treat a successful exit as healthy.
 *
 * Useful for health checks that shell out to a CLI tool (e.g.
 * `bitcoin-cli getblockchaininfo`, `pg_isready`). The command's stdout
 * is passed to the `message` formatter on success.
 *
 * @param runCommand - Command and arguments to execute (e.g. `['pg_isready', '-U', 'postgres']`)
 * @param subcontainer - The subcontainer to run the command in
 * @param options.timeout - Maximum time (ms) to wait before reporting failure (defaults to 30000)
 * @param options.errorMessage - Message shown in the UI if the command fails or times out
 * @param options.message - Function that formats the success message from the command's stdout
 */
export const runHealthScript = async <Manifest extends SDKManifest>(
  runCommand: string[],
  subcontainer: SubContainer<Manifest>,
  {
    timeout = 30000,
    errorMessage = `Error while running command: ${runCommand}`,
    message = (res: string) =>
      `Have ran script ${runCommand} and the result: ${res}`,
  } = {},
): Promise<HealthCheckResult> => {
  const deadline = timeoutPromise(timeout)
  const res = await Promise.race([subcontainer.execFail(runCommand), deadline])
    .finally(deadline.cancel)
    .catch(e => {
      console.warn(errorMessage)
      console.warn(JSON.stringify(e))
      console.warn(e.toString())
      throw { result: 'failure', message: errorMessage } as HealthCheckResult
    })
  return {
    result: 'success',
    message: message(res.stdout.toString()),
  } as HealthCheckResult
}
