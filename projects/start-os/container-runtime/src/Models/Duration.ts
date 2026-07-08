import { z } from '@start9labs/start-sdk'

export type TimeUnit = 'd' | 'h' | 's' | 'ms' | 'm' | 'µs' | 'ns'
export type Duration = `${number}${TimeUnit}`

const durationRegex = /^([0-9]*(\.[0-9]+)?)(ns|µs|ms|s|m|d)$/

export const matchDuration = z.string().refine(isDuration)
export function isDuration(value: string): value is Duration {
  return durationRegex.test(value)
}

export function duration(timeValue: number, timeUnit: TimeUnit = 's') {
  return `${timeValue > 0 ? timeValue : 0}${timeUnit}` as Duration
}
const unitsToSeconds: Record<string, number> = {
  ns: 1e-9,
  µs: 1e-6,
  ms: 0.001,
  s: 1,
  m: 60,
  h: 3600,
  d: 86400,
}

export function fromDuration(duration: Duration | number): number {
  if (typeof duration === 'number') return duration
  const [, num, , unit] = duration.match(durationRegex) || []
  return Number(num) * unitsToSeconds[unit]
}
