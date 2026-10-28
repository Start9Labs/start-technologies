import { TriggerInput } from './TriggerInput'
export { cooldownTrigger } from './cooldownTrigger'
export { onceTrigger } from './onceTrigger'
export { statusTrigger } from './statusTrigger'

/**
 * A function that controls the polling interval of a health check.
 *
 * A trigger is an async iterator factory: it receives a `getInput` callback
 * that returns the latest {@link TriggerInput} (including the most recent
 * health check result), and yields once each time the health check should
 * run again. The time between yields determines the polling frequency.
 *
 * Most packages should use the built-in triggers (`sdk.trigger.*`) rather
 * than implementing this interface directly.
 */
export type Trigger = (
  getInput: () => TriggerInput,
) => AsyncIterator<unknown, unknown, never>
