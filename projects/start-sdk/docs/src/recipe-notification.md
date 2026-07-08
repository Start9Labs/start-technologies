# Post a Notification to the User

Surface information to the user in the StartOS notifications panel — the same panel where StartOS shows backup-completion notices, install failures, and similar OS-generated events. Use this **sparingly**, only for information the user genuinely needs to know about — most commonly that a long-running action has finished (a sync health check that finally passes, a lengthy reindex or migration completing). Notifications are not a changelog feed or an activity log. If you need the user to _do_ something, use a [Task](tasks.md) instead.

## Solution

Call `sdk.notification.create(effects, options)` from any context that has `effects` (init, main, an action handler, a health-check body). `options` is `{ level, title, message, data? }`. Omit `data` for a plain panel entry; pass markdown text for `data` to attach a long-form body that the UI renders in a "View Details" modal — use this for a completion summary or a structured error report, not for short status strings. The host attributes the notification to the calling service automatically; a package cannot post on behalf of another package.

Notifications are not idempotent — every call creates a new entry. Gate posts behind a one-shot condition (a flag in your store, a health check flipping to passing, etc.) so a polling loop doesn't fill the panel.

**Reference:** [Notifications](notifications.md) · [Tasks](tasks.md) (when the user must act)

## Examples

See the [Notifications](notifications.md) reference page for code samples covering the common patterns: a one-shot success notice when a sync completes, and a recoverable-error report with markdown details.
