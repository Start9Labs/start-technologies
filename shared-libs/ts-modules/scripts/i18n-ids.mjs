/** Throws for repeated IDs. */
export function uniqueDictionaryIds(values) {
  const ids = new Set()
  const duplicates = new Set()
  for (const id of values) {
    if (ids.has(id)) duplicates.add(id)
    ids.add(id)
  }
  if (duplicates.size)
    throw new Error(`Duplicate i18n IDs: ${[...duplicates].join(', ')}`)
  return ids
}
