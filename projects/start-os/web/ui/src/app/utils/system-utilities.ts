import { inject } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { i18nKey } from '@start9labs/shared'

import { BadgeService } from 'src/app/services/badge.service'

export const SYSTEM_UTILITIES: Record<
  string,
  { icon: string; title: i18nKey }
> = {
  services: {
    icon: '@tui.package',
    title: 'Services',
  },
  marketplace: {
    icon: '@tui.shopping-cart',
    title: 'Marketplace',
  },
  sideload: {
    icon: '@tui.upload',
    title: 'Sideload',
  },
  updates: {
    icon: '@tui.globe',
    title: 'Updates',
  },
  metrics: {
    icon: '@tui.activity',
    title: 'Metrics',
  },
  system: {
    icon: '@tui.settings',
    title: 'System',
  },
  notifications: {
    icon: '@tui.bell',
    title: 'Notifications',
  },
}

export function getMenu() {
  const badge = inject(BadgeService)

  return Object.keys(SYSTEM_UTILITIES).map(key => ({
    name: SYSTEM_UTILITIES[key]?.title || ('' as i18nKey),
    icon: SYSTEM_UTILITIES[key]?.icon || '',
    routerLink: key,
    badge: toSignal(badge.getCount(key), { initialValue: 0 }),
  }))
}
