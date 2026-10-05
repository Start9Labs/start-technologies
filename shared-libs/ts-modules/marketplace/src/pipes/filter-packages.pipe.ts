import { Pipe, PipeTransform } from '@angular/core'
import { T } from '@start9labs/start-core'

import { MarketplacePkg } from '../types'

@Pipe({
  name: 'filterPackages',
})
export class FilterPackagesPipe implements PipeTransform {
  transform = filterPackages
}

const EXACT = 1
const PREFIX = 0.8
const WORD = 0.6
const INFIX = 0.4

const flatten = (value: T.LocaleString) =>
  typeof value === 'string' ? value : Object.values(value).join(' ')

const FIELDS = [
  { get: (pkg: MarketplacePkg) => pkg.title, weight: 1, min: INFIX },
  { get: (pkg: MarketplacePkg) => pkg.id, weight: 0.8, min: INFIX },
  {
    get: (pkg: MarketplacePkg) => flatten(pkg.description.short),
    weight: 0.4,
    min: WORD,
  },
  {
    get: (pkg: MarketplacePkg) => flatten(pkg.description.long),
    weight: 0.2,
    min: WORD,
  },
]

export function filterPackages(
  packages: MarketplacePkg[],
  query: string | null = '',
  category: string | null = '',
  sort: string | null = '',
): MarketplacePkg[] {
  const search = query?.trim().toLowerCase() || ''

  if (search) {
    return packages
      .map(pkg => ({ pkg, score: score(pkg, search) }))
      .filter(match => match.score > 0)
      .sort(
        (a, b) => b.score - a.score || a.pkg.title.localeCompare(b.pkg.title),
      )
      .map(match => match.pkg)
  }

  return packages
    .filter(p => category === 'all' || p.categories.includes(category!))
    .sort((a, b) => {
      switch (sort) {
        case 'a':
          return a.title.localeCompare(b.title)
        case 'z':
          return b.title.localeCompare(a.title)
        default:
          return (
            new Date(b.s9pks[0]?.[1].publishedAt!).valueOf() -
            new Date(a.s9pks[0]?.[1].publishedAt!).valueOf()
          )
      }
    })
    .map(a => ({ ...a }))
}

function score(pkg: MarketplacePkg, search: string): number {
  const words = search.split(/\s+/).map(word => best(pkg, word))

  if (words.includes(0)) return 0

  return Math.max(
    best(pkg, search),
    words.reduce((sum, word) => sum + word) / words.length,
  )
}

function best(pkg: MarketplacePkg, search: string): number {
  return FIELDS.reduce((max, { get, weight, min }) => {
    const match = quality(get(pkg), search)

    return match < min ? max : Math.max(max, weight * match)
  }, 0)
}

function quality(text: string, search: string): number {
  const lower = text.toLowerCase()

  if (lower === search) return EXACT
  if (lower.startsWith(search)) return PREFIX

  let found = 0

  for (
    let i = lower.indexOf(search);
    i !== -1;
    i = lower.indexOf(search, i + 1)
  ) {
    if (!/[a-z0-9]/.test(lower.charAt(i - 1))) return WORD

    found = INFIX
  }

  return found
}
