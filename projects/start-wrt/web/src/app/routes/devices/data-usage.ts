export const BYTES_PER_KB = 1024
export const BYTES_PER_MB = 1024 * 1024
export const BYTES_PER_GB = 1024 * 1024 * 1024
export const BYTES_PER_TB = 1024 * 1024 * 1024 * 1024

const CHART_UNITS = [
  { suffix: 'B', divisor: 1 },
  { suffix: 'KB', divisor: BYTES_PER_KB },
  { suffix: 'MB', divisor: BYTES_PER_MB },
  { suffix: 'GB', divisor: BYTES_PER_GB },
  { suffix: 'TB', divisor: BYTES_PER_TB },
] as const

const NICE_MULTIPLIERS = [1, 1.5, 2, 2.5, 3, 4, 5, 6, 8, 10] as const

const formatters = new Map<string, Intl.NumberFormat>()

function getFormatter(locale: string, digits: number): Intl.NumberFormat {
  const key = `${locale}:${digits}`
  let fmt = formatters.get(key)
  if (!fmt) {
    fmt = new Intl.NumberFormat(locale, {
      minimumFractionDigits: digits,
      maximumFractionDigits: digits,
    })
    formatters.set(key, fmt)
  }
  return fmt
}

export function formatDataUsageMb(
  bytes: number | null | undefined,
  locale = 'en-US',
): string | null {
  if (bytes == null || bytes <= 0) {
    return null
  }

  const mb = bytes / BYTES_PER_MB
  if (mb < 0.1) {
    return `< ${getFormatter(locale, 1).format(0.1)}`
  }

  if (Math.round(mb * 10) >= 1000) {
    return getFormatter(locale, 0).format(Math.round(mb))
  }

  return getFormatter(locale, 1).format(mb)
}

export interface ChartYAxis {
  maxBytes: number
  labels: string[]
}

function niceStep(raw: number): number {
  const exp = Math.floor(Math.log10(raw))
  const base = 10 ** exp
  const norm = raw / base
  const multiplier =
    NICE_MULTIPLIERS.find(m => m >= norm - 1e-12) ??
    NICE_MULTIPLIERS[NICE_MULTIPLIERS.length - 1]
  return multiplier * base
}

function stepFractionDigits(step: number): number {
  if (Math.abs(step - Math.round(step)) < 1e-9) return 0
  if (Math.abs(step * 10 - Math.round(step * 10)) < 1e-9) return 1
  return 2
}

export function computeChartYAxis(
  points: readonly { download: number; upload: number }[],
  locale = 'en-US',
): ChartYAxis {
  let maxBytes = 0
  for (const p of points) {
    if (p.download > maxBytes) maxBytes = p.download
    if (p.upload > maxBytes) maxBytes = p.upload
  }

  if (maxBytes <= 0) {
    return { maxBytes: 0, labels: ['0'] }
  }

  let unitIndex = CHART_UNITS.length - 1
  while (unitIndex > 0 && maxBytes < CHART_UNITS[unitIndex].divisor) {
    unitIndex -= 1
  }

  let step = niceStep(maxBytes / CHART_UNITS[unitIndex].divisor / 3)
  if (unitIndex === 0) step = Math.ceil(step)
  if (unitIndex < CHART_UNITS.length - 1 && step * 3 >= 1024) {
    unitIndex += 1
    step = niceStep(maxBytes / CHART_UNITS[unitIndex].divisor / 3)
  }

  const unit = CHART_UNITS[unitIndex]
  const fmt = getFormatter(locale, stepFractionDigits(step))

  return {
    maxBytes: Math.round(step * 3 * unit.divisor),
    labels: [
      '0',
      `${fmt.format(step)} ${unit.suffix}`,
      `${fmt.format(step * 2)} ${unit.suffix}`,
      `${fmt.format(step * 3)} ${unit.suffix}`,
    ],
  }
}

export function buildChartLines(
  points: readonly { download: number; upload: number }[],
  maxBytes: number,
  width: number,
  height: number,
): [number, number][][] {
  if (points.length === 0) return []

  const scale = maxBytes > 0 ? height / maxBytes : 1

  if (points.length === 1) {
    const [p] = points
    return [
      [
        [0, p.download * scale],
        [width, p.download * scale],
      ],
      [
        [0, p.upload * scale],
        [width, p.upload * scale],
      ],
    ]
  }

  const len = points.length - 1
  return [
    points.map((p, i): [number, number] => [
      (i / len) * width,
      p.download * scale,
    ]),
    points.map((p, i): [number, number] => [
      (i / len) * width,
      p.upload * scale,
    ]),
  ]
}
