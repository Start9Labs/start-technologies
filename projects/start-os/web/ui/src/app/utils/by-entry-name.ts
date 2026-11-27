import { KeyValue } from '@angular/common'

/** `keyvalue` order by the value's name, then its key. */
export function byEntryName(
  a: KeyValue<string, { name: string }>,
  b: KeyValue<string, { name: string }>,
): number {
  return a.value.name.localeCompare(b.value.name) || a.key.localeCompare(b.key)
}
