import { i18nPipe } from '@start9labs/shared'
import { ISB, T } from '@start9labs/start-core'

type ServerValue = {
  upsName: string
  driver: string
  port: string
  monitorUsername: string
  monitorPassword: string | null
  listenAll: boolean
  remoteUsername: string | null
  remotePassword: string | null
  shutdownDelay: number
}

type ClientValue = {
  upsName: string
  host: string
  port: number
  monitorUsername: string
  monitorPassword: string | null
  shutdownDelay: number
}

export type NutForm = {
  enabled: boolean
  settings:
    | { selection: 'server'; value: ServerValue }
    | { selection: 'client'; value: ClientValue }
}

const DEFAULT_SERVER: ServerValue = {
  upsName: 'ups',
  driver: 'usbhid-ups',
  port: 'auto',
  monitorUsername: 'monuser',
  monitorPassword: null,
  listenAll: false,
  remoteUsername: null,
  remotePassword: null,
  shutdownDelay: 5,
}

export const DEFAULT_NUT_CONFIG: T.NutConfig = {
  enabled: false,
  settings: null,
}

export function nutSpec(i18n: i18nPipe) {
  return ISB.InputSpec.of({
    enabled: ISB.Value.toggle({
      name: i18n.transform('Enabled'),
      default: false,
    }),
    settings: ISB.Value.union({
      name: i18n.transform('Mode'),
      default: 'server',
      variants: ISB.Variants.of({
        server: {
          name: i18n.transform('Direct UPS'),
          spec: ISB.InputSpec.of({
            upsName: ISB.Value.text({
              name: i18n.transform('UPS Name'),
              required: true,
              default: 'ups',
              placeholder: 'ups',
            }),
            driver: ISB.Value.text({
              name: i18n.transform('Driver'),
              required: true,
              default: 'usbhid-ups',
              placeholder: 'usbhid-ups',
            }),
            port: ISB.Value.text({
              name: i18n.transform('Device or address'),
              required: true,
              default: 'auto',
              placeholder: 'auto',
            }),
            monitorUsername: ISB.Value.text({
              name: i18n.transform('Monitor username'),
              required: true,
              default: 'monuser',
              placeholder: 'monuser',
            }),
            monitorPassword: ISB.Value.text({
              name: i18n.transform('Monitor password'),
              required: true,
              default: null,
              masked: true,
            }),
            listenAll: ISB.Value.toggle({
              name: i18n.transform('Allow network clients'),
              default: false,
            }),
            remoteUsername: ISB.Value.text({
              name: i18n.transform('Network client username'),
              required: false,
              default: null,
              placeholder: 'upsclient',
            }),
            remotePassword: ISB.Value.text({
              name: i18n.transform('Network client password'),
              required: false,
              default: null,
              masked: true,
            }),
            shutdownDelay: ISB.Value.number({
              name: i18n.transform('Shutdown delay'),
              required: true,
              default: 5,
              integer: true,
              min: 0,
              max: 300,
              units: i18n.transform('Seconds'),
            }),
          }),
        },
        client: {
          name: i18n.transform('Network UPS client'),
          spec: ISB.InputSpec.of({
            upsName: ISB.Value.text({
              name: i18n.transform('UPS Name'),
              required: true,
              default: 'ups',
              placeholder: 'ups',
            }),
            host: ISB.Value.text({
              name: i18n.transform('NUT server host'),
              required: true,
              default: null,
              placeholder: '192.168.1.10',
            }),
            port: ISB.Value.number({
              name: i18n.transform('NUT server port'),
              required: true,
              default: 3493,
              integer: true,
              min: 1,
              max: 65535,
            }),
            monitorUsername: ISB.Value.text({
              name: i18n.transform('Monitor username'),
              required: true,
              default: 'monuser',
              placeholder: 'monuser',
            }),
            monitorPassword: ISB.Value.text({
              name: i18n.transform('Monitor password'),
              required: true,
              default: null,
              masked: true,
            }),
            shutdownDelay: ISB.Value.number({
              name: i18n.transform('Shutdown delay'),
              required: true,
              default: 5,
              integer: true,
              min: 0,
              max: 300,
              units: i18n.transform('Seconds'),
            }),
          }),
        },
      }),
    }),
  })
}

export function toNutForm(config: T.NutConfig): NutForm {
  return {
    enabled: config.enabled,
    settings: toSettingsForm(config.settings),
  }
}

export function toNutConfig(value: NutForm): T.NutConfig {
  return {
    enabled: value.enabled,
    settings: toNutSettings(value.settings),
  }
}

function toSettingsForm(settings: T.NutSettings | null): NutForm['settings'] {
  switch (settings?.mode) {
    case 'client':
      return { selection: 'client', value: settings }
    case 'server':
      return { selection: 'server', value: settings }
    default:
      return { selection: 'server', value: DEFAULT_SERVER }
  }
}

function toNutSettings(settings: NutForm['settings']): T.NutSettings {
  switch (settings.selection) {
    case 'client':
      return {
        mode: 'client',
        ...settings.value,
        monitorPassword: settings.value.monitorPassword || '',
      }
    case 'server':
      return {
        mode: 'server',
        ...settings.value,
        monitorPassword: settings.value.monitorPassword || '',
        remoteUsername: settings.value.remoteUsername || null,
        remotePassword: settings.value.remotePassword || null,
      }
  }
}
