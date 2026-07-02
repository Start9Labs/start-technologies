/** Convert quarter-hour index (0-96) to "HH:MM" string */
export function quarterHourToTime(quarterHours: number): string {
  return `${Math.floor(quarterHours / 4)
    .toString()
    .padStart(2, '0')}:${((quarterHours % 4) * 15).toString().padStart(2, '0')}`
}

/** Convert "HH:MM" string to quarter-hour index (0-96) */
export function timeToQuarterHour(formatted: string): number {
  const [h, m] = formatted.split(':').map(Number)

  return h * 4 + Math.round(m / 15)
}

/** Format "HH:MM" (24h) to "h:MMam/pm" (12h) */
export function formatTime12h(time: string): string {
  const [h, m] = time.split(':').map(Number)
  const period = h >= 12 ? 'pm' : 'am'
  const hour = h % 12 || 12

  return h === 24
    ? `12:00am`
    : `${hour}:${m.toString().padStart(2, '0')}${period}`
}

type ScheduleWindowLike = {
  startTime: string
  endTime: string
  days: readonly boolean[]
}

const DAY = 24 * 60
const WEEK = 7 * DAY

/**
 * Expand each (window, active day) into half-open minute segments on the weekly
 * timeline [0, WEEK), splitting any that cross the week boundary. `endTime <=
 * startTime` denotes a window that spans past midnight (`end < start` wraps;
 * `end == start` is a full 24-hour window). Shared by `windowsOverlap` and
 * `coversFullWeek`.
 */
function toWeekSegments(windows: ScheduleWindowLike[]): [number, number][] {
  const min = (t: string) => {
    const [h, m] = t.split(':').map(Number)
    return h * 60 + m
  }
  const segs: [number, number][] = []
  for (const w of windows) {
    const start = min(w.startTime)
    const end = min(w.endTime)
    w.days.forEach((on, d) => {
      if (!on) return
      const base = d * DAY
      const s = base + start
      const e = base + (end <= start ? end + DAY : end)
      if (e <= WEEK) {
        segs.push([s, e])
      } else {
        segs.push([s, WEEK])
        segs.push([0, e - WEEK])
      }
    })
  }
  return segs
}

/**
 * True if any two schedule windows occupy overlapping time on the weekly
 * timeline. Mirrors the backend `windows_overlap` check so the FE can warn before
 * submitting. Each window is `{ startTime, endTime, days[Sun..Sat] }`.
 */
export function windowsOverlap(windows: ScheduleWindowLike[]): boolean {
  const segs = toWeekSegments(windows)
  for (let i = 0; i < segs.length; i++) {
    for (let j = i + 1; j < segs.length; j++) {
      if (segs[i][0] < segs[j][1] && segs[j][0] < segs[i][1]) return true
    }
  }
  return false
}

/**
 * True if the windows tile the entire week with no gap. Such a schedule produces
 * no cron edges, so the backend rejects it (`covers_full_week`); the FE warns
 * before submitting. Each window is `{ startTime, endTime, days[Sun..Sat] }`.
 */
export function coversFullWeek(windows: ScheduleWindowLike[]): boolean {
  const segs = toWeekSegments(windows).sort((a, b) => a[0] - b[0])
  let covered = 0
  for (const [s, e] of segs) {
    if (s > covered) return false // gap before this segment
    if (e > covered) covered = e
    if (covered >= WEEK) return true
  }
  return covered >= WEEK
}
