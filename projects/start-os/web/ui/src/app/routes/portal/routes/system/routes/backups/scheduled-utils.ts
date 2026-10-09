import type { T } from '@start9labs/start-core'

export type BackupScheduleFrequency = 'hourly' | 'daily' | 'weekly' | 'monthly'
export type BackupRetentionInterval = 'hour' | 'day' | 'week' | 'month'
export type BackupRetentionPeriodLabel =
  | BackupRetentionInterval
  | 'hours'
  | 'days'
  | 'weeks'
  | 'months'

export const SYSTEM_PACKAGE_ID = 'x_system'
export const BACKUP_HOURS = Array.from({ length: 24 }, (_, hour) => hour)
export const BACKUP_MINUTES = Array.from({ length: 60 }, (_, minute) => minute)
export const BACKUP_MONTH_DAYS = Array.from({ length: 31 }, (_, day) => day + 1)
export const BACKUP_FREQUENCIES: readonly BackupScheduleFrequency[] = [
  'hourly',
  'daily',
  'weekly',
  'monthly',
]
export const BACKUP_RETENTION_INTERVALS: readonly BackupRetentionInterval[] = [
  'hour',
  'day',
  'week',
  'month',
]
export const BACKUP_WEEKDAYS = [
  { value: 0, label: 'Sunday' as const },
  { value: 1, label: 'Monday' as const },
  { value: 2, label: 'Tuesday' as const },
  { value: 3, label: 'Wednesday' as const },
  { value: 4, label: 'Thursday' as const },
  { value: 5, label: 'Friday' as const },
  { value: 6, label: 'Saturday' as const },
] as const

export interface BackupTargetName {
  id: string
  name: string
}

export function backupTargetName(
  targets: readonly BackupTargetName[],
  id: string,
): string {
  return targets.find(target => target.id === id)?.name || id
}

export function backupPauseLabel(pause: T.BackupJobPause) {
  switch (pause.reason) {
    case 'targetUnavailable':
      return 'Backup location unavailable' as const
    case 'targetIdentityMismatch':
      return 'Backup location changed' as const
    case 'targetUnreadable':
      return 'Backup location unreadable' as const
    case 'reauthenticationRequired':
      return 'Authentication required' as const
    default:
      return 'Paused' as const
  }
}

export function backupTimezones(current: string): string[] {
  const supported = Intl.supportedValuesOf?.('timeZone') || []
  return [...new Set([current, 'UTC', ...supported].filter(Boolean))].sort()
}

export function formatBackupTime(value: number): string {
  return String(value).padStart(2, '0')
}

export function backupFrequencyLabel(frequency: BackupScheduleFrequency) {
  if (frequency === 'hourly') return 'Hourly' as const
  if (frequency === 'weekly') return 'Weekly' as const
  if (frequency === 'monthly') return 'Monthly' as const
  return 'Daily' as const
}

export function backupWeekdayLabel(weekday: number) {
  return BACKUP_WEEKDAYS[weekday]?.label || ('Sunday' as const)
}

export function backupRetentionIntervalLabel(
  interval: BackupRetentionTierEditor['interval'],
) {
  if (interval === 'hour') return 'Hour' as const
  if (interval === 'day') return 'Day' as const
  if (interval === 'week') return 'Week' as const
  if (interval === 'month') return 'Month' as const
  return 'Custom' as const
}

export function removeBackupRetentionRule<T>(
  primary: T,
  additional: readonly T[],
  index: number,
  replacement: T,
): { primary: T; additional: T[]; keepAdditional: boolean } {
  const rules = [primary, ...additional]
  rules.splice(index, 1)
  return {
    primary: rules[0] || replacement,
    additional: rules.slice(1),
    keepAdditional: !!rules.length,
  }
}

export interface BackupScheduleFormValue {
  frequency: BackupScheduleFrequency
  minute: number
  hour: number
  weekday: number
  dayOfMonth: number
  timezone: string
}

type BackupSummaryLabel =
  | 'Hourly'
  | 'Minute'
  | 'Monthly'
  | 'Day of month'
  | 'Daily'
  | 'Service'
  | 'Services'
  | 'Future services included'
  | 'Future services not included'
  | 'No System data'
  | (typeof BACKUP_WEEKDAYS)[number]['label']

type BackupSummaryTranslator = (label: BackupSummaryLabel) => string

export function formatBackupScheduleSummary(
  form: BackupScheduleFormValue,
  translate: BackupSummaryTranslator,
): string {
  const minute = formatBackupTime(form.minute)
  const time = `${formatBackupTime(form.hour)}:${minute}`
  if (form.frequency === 'hourly') {
    return `${translate('Hourly')} · ${translate('Minute')} ${minute} · ${form.timezone}`
  }
  if (form.frequency === 'weekly') {
    return `${translate(backupWeekdayLabel(form.weekday))} · ${time} · ${form.timezone}`
  }
  if (form.frequency === 'monthly') {
    return `${translate('Monthly')} · ${translate('Day of month')} ${form.dayOfMonth} · ${time} · ${form.timezone}`
  }
  return `${translate('Daily')} · ${time} · ${form.timezone}`
}

export function formatBackupServiceSummary(
  selected: number,
  total: number,
  includeFuture: boolean,
  includesSystem: boolean,
  translate: BackupSummaryTranslator,
): string {
  const serviceLabel = translate(total === 1 ? 'Service' : 'Services')
  const futureLabel = translate(
    includeFuture ? 'Future services included' : 'Future services not included',
  )
  const systemLabel = includesSystem ? '' : ` · ${translate('No System data')}`
  return `${selected} / ${total} ${serviceLabel} · ${futureLabel}${systemLabel}`
}

export interface BackupJobAttentionState {
  enabled: boolean
  pause: { reason: string } | null
  status: { lastResult: string | null }
}

export function backupJobNeedsAttention(job: BackupJobAttentionState): boolean {
  return (
    job.enabled &&
    ((!!job.pause && job.pause.reason !== 'user') ||
      job.status.lastResult === 'failed' ||
      job.status.lastResult === 'partiallyFailed')
  )
}

/** Preserves package IDs absent from the installed-service list. */
export interface BackupServiceSelection {
  packageIds: string[]
  includeFuture: boolean
  preservedSelectedPackageIds: string[]
  preservedExcludedPackageIds: string[]
}

/** Preserves exact custom retention tiers. */
export interface BackupRetentionTierEditor {
  interval: BackupRetentionInterval | 'custom'
  duration: number
  customIntervalHours: number
  customCoverageHours: number
}

export type BackupRetentionRuleValue = Pick<
  BackupRetentionTierEditor,
  'interval' | 'duration'
> &
  Partial<
    Pick<
      BackupRetentionTierEditor,
      'customIntervalHours' | 'customCoverageHours'
    >
  >

export function hasDuplicateRetentionRules(
  rules: readonly BackupRetentionRuleValue[],
): boolean {
  const keys = rules.map(rule => {
    if (rule.interval === 'custom') {
      return `custom:${rule.customIntervalHours}:${rule.customCoverageHours}`
    }
    return `${rule.interval}:${rule.duration}`
  })

  return new Set(keys).size !== keys.length
}

export function isValidBackupRetentionRules(
  rules: readonly BackupRetentionRuleValue[],
): boolean {
  return (
    !hasDuplicateRetentionRules(rules) &&
    rules.every(rule => {
      if (rule.interval === 'custom') {
        return (
          Number(rule.customIntervalHours) > 0 &&
          Number(rule.customCoverageHours) >= Number(rule.customIntervalHours)
        )
      }
      return (
        BACKUP_RETENTION_INTERVALS.includes(rule.interval) &&
        Number.isInteger(rule.duration) &&
        rule.duration >= 1 &&
        rule.duration <= 365
      )
    })
  )
}

export function serializeBackupSchedule(
  form: BackupScheduleFormValue,
): T.Schedule {
  const minute = clampInteger(form.minute, 0, 59)
  const hour = clampInteger(form.hour, 0, 23)
  const weekday = clampInteger(form.weekday, 0, 6)
  const dayOfMonth = clampInteger(form.dayOfMonth, 1, 31)
  const cron =
    form.frequency === 'hourly'
      ? `${minute} * * * *`
      : form.frequency === 'daily'
        ? `${minute} ${hour} * * *`
        : form.frequency === 'weekly'
          ? `${minute} ${hour} * * ${weekday}`
          : `${minute} ${hour} ${dayOfMonth} * *`
  return { cron, timezone: form.timezone }
}

export function isValidBackupSchedule(form: BackupScheduleFormValue): boolean {
  return !!(
    ['hourly', 'daily', 'weekly', 'monthly'].includes(form.frequency) &&
    Number.isInteger(form.minute) &&
    form.minute >= 0 &&
    form.minute <= 59 &&
    (form.frequency === 'hourly' ||
      (Number.isInteger(form.hour) && form.hour >= 0 && form.hour <= 23)) &&
    (form.frequency !== 'monthly' ||
      (Number.isInteger(form.dayOfMonth) &&
        form.dayOfMonth >= 1 &&
        form.dayOfMonth <= 31)) &&
    form.timezone.trim()
  )
}

export function parseBackupSchedule(
  schedule: T.Schedule,
): BackupScheduleFormValue {
  const fields = schedule.cron.trim().split(/\s+/)
  const frequency: BackupScheduleFrequency =
    fields[2] !== '*'
      ? 'monthly'
      : fields[4] !== '*'
        ? 'weekly'
        : fields[1] !== '*'
          ? 'daily'
          : 'hourly'
  return {
    frequency,
    minute: cronFieldStart(fields[0]),
    hour: cronFieldStart(fields[1]),
    weekday: cronFieldStart(fields[4]) % 7,
    dayOfMonth: cronFieldStart(fields[2], 1),
    timezone: schedule.timezone,
  }
}

/** Reads the first value of a validated singleton cron field. */
function cronFieldStart(field = '*', minimum = 0): number {
  const [start = '*'] = field.split(/[,/-]/)
  return Number(start.replace('*', String(minimum)))
}

/** Preserves package IDs absent from the installed-service list. */
export function parseBackupServiceSelection(
  services: T.BackupServiceScope,
  installedPackageIds: string[],
): BackupServiceSelection {
  const installed = new Set(installedPackageIds)
  if (services.type === 'selected') {
    const includeSystem = services.includeSystem !== false
    return {
      packageIds: [
        ...(includeSystem && installed.has(SYSTEM_PACKAGE_ID)
          ? [SYSTEM_PACKAGE_ID]
          : []),
        ...services.packageIds.filter(
          id => id !== SYSTEM_PACKAGE_ID && installed.has(id),
        ),
      ],
      includeFuture: false,
      preservedSelectedPackageIds: services.packageIds.filter(
        id => id !== SYSTEM_PACKAGE_ID && !installed.has(id),
      ),
      preservedExcludedPackageIds: [],
    }
  }
  if (services.type === 'allExcept') {
    const excluded = new Set(services.excludedPackageIds)
    return {
      packageIds: installedPackageIds.filter(id => !excluded.has(id)),
      includeFuture: true,
      preservedSelectedPackageIds: [],
      preservedExcludedPackageIds: services.excludedPackageIds.filter(
        id => !installed.has(id),
      ),
    }
  }
  return {
    packageIds: [...installedPackageIds],
    includeFuture: true,
    preservedSelectedPackageIds: [],
    preservedExcludedPackageIds: [],
  }
}

/** Preserves package IDs absent from the installed-service list. */
export function serializeBackupServiceSelection(
  selection: BackupServiceSelection,
  installedPackageIds: string[],
): T.BackupServiceScope {
  const includeSystem = selection.packageIds.includes(SYSTEM_PACKAGE_ID)
  const selectedPackageIds = selection.packageIds.filter(
    id => id !== SYSTEM_PACKAGE_ID,
  )
  const preservedSelectedPackageIds =
    selection.preservedSelectedPackageIds.filter(id => id !== SYSTEM_PACKAGE_ID)
  if (!selection.includeFuture) {
    return {
      type: 'selected',
      packageIds: [
        ...new Set([...selectedPackageIds, ...preservedSelectedPackageIds]),
      ],
      includeSystem,
    }
  }
  const selected = new Set(selectedPackageIds)
  if (includeSystem) selected.add(SYSTEM_PACKAGE_ID)
  return {
    type: 'allExcept',
    excludedPackageIds: [
      ...new Set([
        ...selection.preservedExcludedPackageIds,
        ...installedPackageIds.filter(id => !selected.has(id)),
      ]),
    ],
  }
}

export function parseBackupRetentionTier(
  tier?: T.RetentionTier,
): BackupRetentionTierEditor {
  if (!tier) {
    return {
      interval: 'day',
      duration: 7,
      customIntervalHours: 24,
      customCoverageHours: 168,
    }
  }
  const interval = retentionIntervalFromSeconds(tier.intervalSeconds)
  const duration = tier.coverageSeconds / tier.intervalSeconds
  const standard =
    retentionIntervalSeconds(interval) === tier.intervalSeconds &&
    Number.isInteger(duration) &&
    duration >= 1 &&
    duration <= 365
  return {
    interval: standard ? interval : 'custom',
    duration: standard ? duration : 7,
    customIntervalHours: tier.intervalSeconds / 3600,
    customCoverageHours: tier.coverageSeconds / 3600,
  }
}

/** Preserves exact custom retention tiers. */
export function serializeBackupRetentionTier(
  editor: BackupRetentionRuleValue,
): T.RetentionTier {
  if (editor.interval === 'custom') {
    const minimumHours = 1 / 3600
    return {
      intervalSeconds: Math.round(
        Math.max(
          minimumHours,
          Number(editor.customIntervalHours) || minimumHours,
        ) * 3600,
      ),
      coverageSeconds: Math.round(
        Math.max(
          minimumHours,
          Number(editor.customCoverageHours) || minimumHours,
        ) * 3600,
      ),
    }
  }
  const intervalSeconds = retentionIntervalSeconds(editor.interval)
  return {
    intervalSeconds,
    coverageSeconds:
      intervalSeconds * Math.max(1, Number(editor.duration) || 1),
  }
}

export function serializeBackupRetentionPolicy(
  rules: readonly BackupRetentionRuleValue[],
): T.RetentionPolicy {
  return { tiers: rules.map(serializeBackupRetentionTier) }
}

export function retentionIntervalSeconds(
  interval: BackupRetentionInterval,
): number {
  if (interval === 'hour') return 60 * 60
  if (interval === 'week') return 7 * 24 * 60 * 60
  if (interval === 'month') return 30 * 24 * 60 * 60
  return 24 * 60 * 60
}

export function retentionIntervalFromSeconds(
  seconds?: number,
): BackupRetentionInterval {
  if (!seconds) return 'day'
  if (seconds < 24 * 60 * 60) return 'hour'
  if (seconds < 7 * 24 * 60 * 60) return 'day'
  if (seconds < 30 * 24 * 60 * 60) return 'week'
  return 'month'
}

export function scheduleNeedsMoreFrequentRuns(
  frequency: BackupScheduleFrequency,
  policies: T.RetentionPolicy[],
): boolean {
  const finestInterval = Math.min(
    ...policies.flatMap(policy =>
      policy.tiers.map(tier => tier.intervalSeconds),
    ),
  )
  if (!Number.isFinite(finestInterval)) return false

  const maximumGapSeconds =
    frequency === 'hourly'
      ? 60 * 60
      : frequency === 'daily'
        ? 24 * 60 * 60
        : frequency === 'weekly'
          ? 7 * 24 * 60 * 60
          : 31 * 24 * 60 * 60
  return finestInterval < maximumGapSeconds
}

export function formatBackupRetentionRule(
  rule: BackupRetentionRuleValue,
  translate: (
    label:
      | BackupRetentionPeriodLabel
      | 'Minute'
      | 'Minutes'
      | 'Keep one backup every'
      | 'for',
  ) => string,
): string {
  const every = translate('Keep one backup every')
  const forLabel = translate('for')
  if (rule.interval === 'custom') {
    const duration = (hours = 0) => {
      const seconds = Math.round(hours * 3600)
      if (seconds % 60) return `${seconds}s`
      const minutes = seconds / 60
      if (minutes % 60) {
        return `${minutes} ${translate(minutes === 1 ? 'Minute' : 'Minutes')}`
      }
      return `${minutes / 60} ${translate(minutes === 60 ? 'hour' : 'hours')}`
    }
    return `${every} ${duration(rule.customIntervalHours)} ${forLabel} ${duration(rule.customCoverageHours)}`
  }
  return `${every} ${translate(rule.interval)} ${forLabel} ${rule.duration} ${translate(retentionPeriodLabel(rule.interval, rule.duration))}`
}

export function retentionPeriodLabel(
  interval: BackupRetentionInterval,
  count: number,
): BackupRetentionPeriodLabel {
  if (count === 1) return interval
  return `${interval}s` as BackupRetentionPeriodLabel
}

function clampInteger(value: number, minimum: number, maximum: number): number {
  return Math.max(minimum, Math.min(maximum, Math.trunc(Number(value) || 0)))
}
