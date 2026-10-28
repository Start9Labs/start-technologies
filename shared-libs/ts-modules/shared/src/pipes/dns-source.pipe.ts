import { Pipe, PipeTransform } from '@angular/core'
import { T } from '@start9labs/start-core'
import { i18nKey } from '../i18n/i18n.providers'

@Pipe({ name: 'dnsSource' })
export class DnsSourcePipe implements PipeTransform {
  transform(source: T.NetworkInterfaceInfo['dns']['source']): i18nKey {
    switch (source) {
      case 'gateway':
        return 'Provided by gateway'
      case 'static':
        return 'Static'
      case 'public-fallback':
        return 'Public fallback (Cloudflare)'
    }
  }
}
