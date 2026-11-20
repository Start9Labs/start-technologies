import type { T } from '@start9labs/start-core'
import { defer, EMPTY, expand, map, toArray } from 'rxjs'

import type { Result } from 'src/app/services/api/api.types'

export type HistoryEntry = T.BackupActivity & { manualRun?: T.BackupRun }

// Nanosecond boundaries distinguish adjacent operations.
function timestamp(value: string): bigint {
  const fraction = value.match(/\.(\d+)/)?.[1] || ''
  return (
    BigInt(Date.parse(value)) * 1_000_000n +
    BigInt(fraction.padEnd(9, '0').slice(3, 9))
  )
}

export function mergeBackupHistory(
  activities: T.BackupActivity[],
  runs: T.BackupRun[],
): HistoryEntry[] {
  const entries: HistoryEntry[] = activities.map(activity => ({ ...activity }))
  const manual = new Map<
    string,
    { entry: HistoryEntry; start: bigint; end: bigint }[]
  >()
  for (const entry of entries) {
    if (entry.kind !== 'manual' || !entry.completedAt) continue
    const target = manual.get(entry.targetId) || []
    target.push({
      entry,
      start: timestamp(entry.startedAt),
      end: timestamp(entry.completedAt),
    })
    manual.set(entry.targetId, target)
  }
  for (const target of manual.values())
    target.sort((a, b) =>
      a.start === b.start ? 0 : a.start < b.start ? -1 : 1,
    )

  for (const run of runs) {
    const target = manual.get(run.targetId) || []
    const start = timestamp(run.startedAt)
    let low = 0
    let high = target.length
    while (low < high) {
      const middle = (low + high) >>> 1
      if (target[middle]!.start <= start) low = middle + 1
      else high = middle
    }
    const match = target[low - 1]
    // Serialized manual activities enclose their reports' timestamps.
    if (
      match &&
      !match.entry.manualRun &&
      match.end >= timestamp(run.completedAt)
    ) {
      match.entry.manualRun = run
      continue
    }
    const errors = [
      run.report.server.error,
      ...Object.values(run.report.packages).map(report => report.error),
      ...Object.values(run.report.sharedVolumes).map(volume => volume.error),
    ]
    const successes =
      !run.report.server.error ||
      Object.values(run.report.packages).some(report => !report.error) ||
      Object.values(run.report.sharedVolumes).some(volume => !volume.error)
    entries.push({
      id: `manual:${run.id}`,
      kind: 'manual',
      state: errors.some(Boolean)
        ? successes
          ? 'partiallyFailed'
          : 'failed'
        : 'succeeded',
      targetId: run.targetId,
      sourceServerId: null,
      jobId: null,
      jobName: null,
      trigger: null,
      startedAt: run.startedAt,
      completedAt: run.completedAt,
      intendedServices: Object.keys(run.report.packages),
      services: run.report.packages,
      error: run.report.server.error,
      manualRun: run,
    })
  }
  return entries.sort((a, b) => {
    const first = timestamp(a.startedAt)
    const second = timestamp(b.startedAt)
    return first === second ? a.id.localeCompare(b.id) : first > second ? -1 : 1
  })
}

export function readManualHistory(
  fetchPage: (params: {
    offset: number
    limit: number
  }) => Promise<Result<'backup.history.list'>>,
) {
  const load = (offset: number) =>
    defer(() => fetchPage({ offset, limit: 200 })).pipe(
      map(page => ({ ...page, offset })),
    )
  return load(0).pipe(
    expand(page => {
      const next = page.offset + page.runs.length
      if (next >= page.total) return EMPTY
      if (!page.runs.length)
        throw new Error('Incomplete backup history response')
      return load(next)
    }),
    toArray(),
    map(pages => [
      ...new Map(
        pages.flatMap(page => page.runs).map(run => [run.id, run]),
      ).values(),
    ]),
  )
}
