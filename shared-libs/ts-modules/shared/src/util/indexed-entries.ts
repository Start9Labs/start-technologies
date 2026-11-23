type Indexed = { index?: number | null }

/** Orders items by ascending `index`, unindexed last, ties by the given key. */
export function byIndex<T extends Indexed>(key: (item: T) => string) {
  return (a: T, b: T): number => {
    const aKey = key(a)
    const bKey = key(b)
    return (
      (a.index ?? Infinity) - (b.index ?? Infinity) ||
      (aKey < bKey ? -1 : aKey > bKey ? 1 : 0)
    )
  }
}

/** A record's entries in {@link byIndex} order, keyed by record key. Nullish values are skipped. */
export function indexedEntries<T extends Indexed>(
  record: Record<string, T | null | undefined>,
): [string, T][] {
  const byValue = byIndex<T & { key: string }>(item => item.key)
  return Object.entries(record)
    .filter((entry): entry is [string, T] => entry[1] != null)
    .sort(([aKey, a], [bKey, b]) =>
      byValue({ ...a, key: aKey }, { ...b, key: bKey }),
    )
}
