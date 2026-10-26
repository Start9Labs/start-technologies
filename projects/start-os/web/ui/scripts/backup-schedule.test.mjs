import assert from 'node:assert/strict'
import { test } from 'node:test'

import {
  parseBackupSchedule,
  serializeBackupSchedule,
} from '../src/app/routes/portal/routes/system/routes/backups/scheduled-utils.ts'

const schedules = [
  ['0 3 * * 7', '0 3 * * 0'],
  ['0 3 * * 0,7', '0 3 * * 0'],
  ['0 3 * * 7-7', '0 3 * * 0'],
  ['15-15 3 * * *', '15 3 * * *'],
  ['15,15 3 * * *', '15 3 * * *'],
  ['15/60 3 * * *', '15 3 * * *'],
  ['*/60 3 * * *', '0 3 * * *'],
  ['15 3-3 * * *', '15 3 * * *'],
  ['15 */24 * * *', '15 0 * * *'],
  ['15 3 31-31 * *', '15 3 31 * *'],
  ['15 3 */31 * *', '15 3 1 * *'],
  ['15 * * * *', '15 * * * *'],
  [' 15 3 * * * ', '15 3 * * *'],
  ['15\t3\t*\t*\t7', '15 3 * * 0'],
]

for (const [cron, expected] of schedules) {
  test(`UI editing preserves CLI schedule ${JSON.stringify(cron)}`, () => {
    const schedule = { cron, timezone: 'America/New_York' }
    assert.deepEqual(serializeBackupSchedule(parseBackupSchedule(schedule)), {
      cron: expected,
      timezone: schedule.timezone,
    })
  })
}
