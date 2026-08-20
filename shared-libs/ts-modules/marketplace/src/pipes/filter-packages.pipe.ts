import { Pipe, PipeTransform } from '@angular/core'
import Fuse from 'fuse.js'
import { MarketplacePkg } from '../types'

@Pipe({
  name: 'filterPackages',
})
export class FilterPackagesPipe implements PipeTransform {
  transform = filterPackages
}

export function filterPackages(
  packages: MarketplacePkg[],
  query: string | null = '',
  category: string | null = '',
  sort: string | null = '',
): MarketplacePkg[] {
  query = query?.trim() || ''

  // query
  if (query) {
    let options: Fuse.IFuseOptions<MarketplacePkg> = {
      includeScore: true,
      includeMatches: true,
    }

    if (query.length < 4) {
      options = {
        ...options,
        threshold: 0.2,
        location: 0,
        distance: 16,
        keys: [
          {
            name: 'title',
            weight: 1,
          },
          {
            name: 'id',
            weight: 0.5,
          },
        ],
      }
    } else {
      options = {
        ...options,
        ignoreLocation: true,
        useExtendedSearch: true,
        keys: [
          {
            name: 'title',
            weight: 1,
          },
          {
            name: 'id',
            weight: 0.5,
          },
          {
            name: 'description.short',
            weight: 0.4,
          },
          {
            name: 'description.long',
            weight: 0.1,
          },
        ],
      }
      query = `'${query}`
    }

    const fuse = new Fuse(packages, options)
    return fuse.search(query).map(p => p.item)
  }

  // category
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
