import { i18nKey } from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import { PackageDataEntry } from 'src/app/services/patch-db/data-model'
import { getLiveTasks } from 'src/app/utils/get-package-data'

export const INACTIVE_STATUSES: PrimaryStatus[] = [
  'installing',
  'updating',
  'removing',
  'restoring',
  'backing-up',
  'error',
]

export const ALLOWED_STATUSES: Record<T.AllowedStatuses, Set<string>> = {
  'only-running': new Set(['running']),
  'only-stopped': new Set(['stopped']),
  any: new Set([
    'running',
    'stopped',
    'restarting',
    'restoring',
    'stopping',
    'starting',
    'backing-up',
    'task-required',
  ]),
}

export interface PackageStatus {
  primary: PrimaryStatus
  health: T.HealthStatus | null
}

export function renderPkgStatus(pkg: PackageDataEntry): PackageStatus {
  let primary: PrimaryStatus
  let health: T.HealthStatus | null = null

  if (pkg.stateInfo.state === 'installed') {
    primary = getInstalledPrimaryStatus(pkg)
    health = getHealthStatus(pkg.statusInfo)
  } else {
    primary = pkg.stateInfo.state
  }

  return { primary, health }
}

export function getInstalledBaseStatus(statusInfo: T.StatusInfo): BaseStatus {
  if (statusInfo.error) {
    return 'error'
  }

  if (
    statusInfo.desired.main === 'running' &&
    (!statusInfo.started ||
      Object.values(statusInfo.health)
        .filter(h => !!h)
        .some(h => h.result === 'starting' || h.result === 'waiting'))
  ) {
    return 'starting'
  }

  if (statusInfo.desired.main === 'stopped' && statusInfo.started) {
    return 'stopping'
  }

  return statusInfo.desired.main
}

export function getInstalledPrimaryStatus(
  pkg: PackageDataEntry,
): PrimaryStatus {
  const base = getInstalledBaseStatus(pkg.statusInfo)

  if (
    !INACTIVE_STATUSES.includes(base) &&
    getLiveTasks(pkg).some(
      ([_, t]) => t.active && t.task.severity === 'critical',
    )
  ) {
    return 'task-required'
  }

  return base
}

function getHealthStatus(statusInfo: T.StatusInfo): T.HealthStatus | null {
  if (statusInfo.desired.main !== 'running') {
    return null
  }

  const values = Object.values(statusInfo.health).filter(h => !!h)

  if (values.some(h => h.result === 'failure')) {
    return 'failure'
  }

  if (values.some(h => h.result === 'starting')) {
    return 'starting'
  }

  if (values.some(h => h.result === 'loading')) {
    return 'loading'
  }

  return 'success'
}

export interface StatusRendering {
  display: i18nKey
  color: string
  showDots?: boolean
}

export type BaseStatus =
  | 'installing'
  | 'updating'
  | 'removing'
  | 'restoring'
  | 'starting'
  | 'running'
  | 'stopping'
  | 'restarting'
  | 'stopped'
  | 'backing-up'
  | 'error'

export type PrimaryStatus = BaseStatus | 'task-required'

export type DependencyStatus = 'warning' | 'satisfied'

export const PrimaryRendering: Record<PrimaryStatus, StatusRendering> = {
  installing: {
    display: 'Installing',
    color: 'primary',
    showDots: true,
  },
  updating: {
    display: 'Updating',
    color: 'primary',
    showDots: true,
  },
  removing: {
    display: 'Removing',
    color: 'danger',
    showDots: true,
  },
  restoring: {
    display: 'Restoring',
    color: 'primary',
    showDots: true,
  },
  stopping: {
    display: 'Stopping',
    color: 'dark-shade',
    showDots: true,
  },
  restarting: {
    display: 'Restarting',
    color: 'tertiary',
    showDots: true,
  },
  stopped: {
    display: 'Stopped',
    color: 'dark-shade',
    showDots: false,
  },
  'backing-up': {
    display: 'Backing Up',
    color: 'primary',
    showDots: true,
  },
  starting: {
    display: 'Starting',
    color: 'primary',
    showDots: true,
  },
  running: {
    display: 'Running',
    color: 'success',
    showDots: false,
  },
  'task-required': {
    display: 'Task Required',
    color: 'warning',
    showDots: false,
  },
  error: {
    display: 'Service Launch Error',
    color: 'danger',
    showDots: false,
  },
}

// The CSS color a primary status renders in — shared so a status reads the same
// everywhere (the services-list status, the Address Requirements note, ...).
export function getStatusColor(primary: PrimaryStatus): string {
  switch (primary) {
    case 'running':
      return 'var(--tui-status-positive)'
    case 'task-required':
      return 'var(--tui-status-warning)'
    case 'error':
      return 'var(--tui-status-negative)'
    case 'stopped':
      return 'var(--tui-text-secondary)'
    default:
      return 'var(--tui-status-info)'
  }
}
