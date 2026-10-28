import { i18nKey, i18nPipe } from '@start9labs/shared'
import { ISB } from '@start9labs/start-core'

import { configBuilderToSpec } from './configBuilderToSpec'
import {
  dnsSelectionServers,
  dnsSelectionSpec,
  dnsSelectionValue,
} from './dns-spec'

const DEFAULT_GATEWAY = '/default'

export function outboundSettingsSpec(
  i18n: i18nPipe,
  gateways: Record<string, string>,
  defaultName: i18nKey,
) {
  return ISB.InputSpec.of({
    gateway: ISB.Value.select<Record<string, string>>({
      name: i18n.transform('Outbound Gateway'),
      default: DEFAULT_GATEWAY,
      values: { [DEFAULT_GATEWAY]: i18n.transform(defaultName), ...gateways },
    }),
    dns: dnsSelectionSpec(i18n),
  })
}

export async function outboundSettingsForm(
  i18n: i18nPipe,
  settings: {
    gateway: string | null
    dns: string[] | null
    options: Record<string, string>
  },
  defaultName: i18nKey,
) {
  return {
    key: JSON.stringify([settings.gateway, settings.dns]),
    spec: await configBuilderToSpec(
      outboundSettingsSpec(i18n, settings.options, defaultName),
    ),
    value: outboundSettingsValue(settings.gateway, settings.dns),
  }
}

type OutboundSettings = ReturnType<typeof outboundSettingsSpec>['_TYPE']

export function outboundSettingsValue(
  gateway: string | null,
  servers: string[] | null,
): OutboundSettings {
  return {
    gateway: gateway ?? DEFAULT_GATEWAY,
    dns: dnsSelectionValue(servers),
  }
}

export function outboundSettingsParams(value: OutboundSettings) {
  return {
    gateway: value.gateway === DEFAULT_GATEWAY ? null : value.gateway,
    dns: { servers: dnsSelectionServers(value.dns) },
  }
}
