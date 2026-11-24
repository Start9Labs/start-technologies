import { T } from '@start9labs/start-core'

import { escapeHtml } from './escape-html'

export function formatProgress({ phases, overall }: T.FullProgress): {
  total: number
  message: string
} {
  return {
    total: getDecimal(leafProgress(overall)),
    message: phases
      .map(p => ({ name: p.name, progress: leafProgress(p.progress) }))
      .filter(
        (
          p,
        ): p is {
          name: string
          progress: false | ProgressDetails
        } => p.progress !== true && p.progress !== null,
      )
      .map(p => `<b>${escapeHtml(p.name)}</b>${getDetails(p.progress)}`)
      .join(', '),
  }
}

/** Scalar `Progress` — every variant except a nested `FullProgress`. */
export type LeafProgress = Exclude<T.Progress, T.FullProgress>

/** Walk through any `Nested` layers and return the scalar `Progress`. */
export function leafProgress(p: T.Progress): LeafProgress {
  let cur = p
  while (cur !== null && typeof cur === 'object' && 'overall' in cur) {
    cur = cur.overall
  }
  return cur as LeafProgress
}

function getDecimal(progress: LeafProgress): number {
  if (progress === true) {
    return 1
  } else if (!progress || !progress.total) {
    return 0
  } else {
    return progress.total && progress.done / progress.total
  }
}

function getDetails(progress: false | ProgressDetails) {
  return progress
    ? `: ${progress.done}/${progress.total} ${progress.units || ''}`
    : ''
}

type ProgressDetails = {
  done: number
  total: number | null
  units: T.ProgressUnits | null
}
