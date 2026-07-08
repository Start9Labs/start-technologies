# Notifications

Notifications are messages your service can post to the StartOS notifications panel — the same panel where StartOS shows backup-completion notices, install failures, and similar OS-generated events. Use them **sparingly**, only for information the user genuinely needs to know about — most commonly that a long-running action has finished: a sync health check that finally passes, a lengthy reindex or migration completing. They are not a changelog feed or an activity log; the vast majority of what your service does should not produce one. If you need the user to _do_ something, use a [Task](./tasks.md) instead.

The host attributes every notification to the calling service automatically — a package cannot post notifications on behalf of another package.

## Plain Notification

Omit `data` for a notification with no extra payload. The notifications panel shows the `title` and `message` directly in the row.

```typescript
await sdk.notification.create(effects, {
  level: 'info',
  title: 'Sync Complete',
  message: 'Initial block download finished.',
})
```

## Notification With Markdown Details

Pass `data` as markdown text when the notification carries long-form content that doesn't belong inline. The panel still shows `title` and `message` in the row, and a "View Details" button opens `data` rendered as markdown in a large modal. Typical uses: a completion summary for a long-running operation, or a diagnostic report for a recoverable error.

`data` should be markdown text — not a short status string.

```typescript
await sdk.notification.create(effects, {
  level: 'success',
  title: 'Reindex Complete',
  message: 'The transaction index finished rebuilding. Tap for a summary.',
  data: ['## Reindex summary', '', '- Blocks processed: 812,043', '- Duration: 3h 14m', '- Index size: 4.2 GiB', '', 'No further action is needed — the service is fully synced.'].join('\n'),
})
```

## Parameters

| Parameter | Type                                          | Description                                                                                                 |
| --------- | --------------------------------------------- | ----------------------------------------------------------------------------------------------------------- |
| `effects` | `Effects`                                     | Provided by the calling context                                                                             |
| `level`   | `'success' \| 'info' \| 'warning' \| 'error'` | Severity, controls the icon and color in the panel                                                          |
| `title`   | `string`                                      | Short headline shown in the row                                                                             |
| `message` | `string`                                      | One-line body shown in the row beneath the title                                                            |
| `data`    | `string \| null` (optional)                   | Optional markdown body rendered in the "View Details" modal. Omit for a plain (panel-row-only) notification |

## Common Patterns

### Notify on Sync Completion

Post a one-time success notification from a daemon's health check or main flow when long-running work finishes:

```typescript
await sdk.notification.create(effects, {
  level: 'success',
  title: i18n('Sync Complete'),
  message: i18n('Bitcoin Core has finished initial block download.'),
})
```

### Report a Recoverable Error With Details

Pair a short `message` with full diagnostic output in `data` so the user gets context without dumping a wall of text into the panel row:

````typescript
await sdk.notification.create(effects, {
  level: 'warning',
  title: i18n('Backup Skipped'),
  message: i18n('A non-critical backup step was skipped. Tap for details.'),
  data: ['## Skipped: optional thumbnail cache', '', '`/data/cache/thumbnails` was not present, so it was skipped during this backup.', 'No data was lost — the cache will be regenerated on next use.', '', '```', err.stack ?? String(err), '```'].join('\n'),
})
````

> [!NOTE]
> Notifications are not idempotent — every call creates a new entry. If a daemon's health loop calls `sdk.notification.create()` on every poll, the panel will fill up. Gate on a one-shot condition (a flag in your store, a state transition, etc.) so you only post when something actually changed.
