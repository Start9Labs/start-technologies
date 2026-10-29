import { T } from '@start9labs/start-core'

/**
 * The value returned from a health check `fn`.
 *
 * Set `result` to one of:
 * - `'success'`  — healthy and fully operational
 * - `'loading'`  — operational but still catching up (e.g. syncing blocks)
 * - `'disabled'` — intentionally inactive (excluded by config)
 * - `'starting'` — not yet ready, still initializing
 * - `'failure'`  — unhealthy, something is wrong
 *
 * Set `waiting: true` on `starting` or `loading` to show a clock instead of
 * a spinner while waiting for an external condition. Defaults to `false`.
 *
 * Every status accepts a `message` string, displayed verbatim in the StartOS UI.
 * Empty or absent messages use the localized status label.
 * Required for `loading` and `failure`; optional for other states.
 */
export type HealthCheckResult = T.NamedHealthCheckResultKindInput
