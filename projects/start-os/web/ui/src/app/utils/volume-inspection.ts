export function isVolumeInspectionStale(
  checkedAt: string | null,
  now = Date.now(),
): boolean {
  return !checkedAt || now - Date.parse(checkedAt) > 10 * 60 * 1000
}
