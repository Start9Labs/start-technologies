import { i18nPipe } from '@start9labs/shared'
import { ISB, T } from '@start9labs/start-core'

export type NutMode = T.NutSettings['mode']

const PASSWORD = { charset: 'a-z,A-Z,0-9', len: 22 }

export const DEFAULT_NUT_CONFIG: T.NutConfig = {
  enabled: false,
  settings: null,
}

export function deviceName({
  vendor,
  product,
  driver,
  serial,
  bus,
  busPort,
}: T.NutDevice) {
  const name = [vendor, product].filter(Boolean).join(' ') || driver
  const identifier = serial || [bus, busPort].filter(Boolean).join(':')
  return identifier ? `${name} (${identifier})` : name
}

function sharingField(i18n: i18nPipe) {
  return {
    sharing: ISB.Value.union({
      name: i18n.transform('Other clients can monitor'),
      description: i18n.transform(
        'Other devices plugged into this UPS can’t see its battery level on their own. Turn this on so they can monitor it through StartOS and shut down safely when its battery runs low. Set them up as NUT clients that connect to this server on port 3493 with the UPS name “startos”.',
      ),
      default: 'off',
      variants: ISB.Variants.of({
        off: { name: i18n.transform('Off'), spec: ISB.InputSpec.of({}) },
        on: {
          name: i18n.transform('On'),
          spec: ISB.InputSpec.of({
            remoteUsername: ISB.Value.text({
              name: i18n.transform('Username'),
              description: i18n.transform(
                'The username other devices use when they connect to this server as NUT clients.',
              ),
              required: true,
              default: 'upsclient',
            }),
            remotePassword: ISB.Value.text({
              name: i18n.transform('Password'),
              description: i18n.transform(
                'The password other devices use when they connect to this server as NUT clients.',
              ),
              required: true,
              default: PASSWORD,
              masked: true,
              generate: PASSWORD,
            }),
          }),
        },
      }),
    }),
  }
}

export function serverSpec(i18n: i18nPipe) {
  return ISB.InputSpec.of(sharingField(i18n))
}

export function deviceServerSpec(
  i18n: i18nPipe,
  devices: readonly T.NutDevice[],
) {
  return ISB.InputSpec.of({
    device: ISB.Value.select({
      name: i18n.transform('UPS'),
      description: i18n.transform(
        'The UPS connected to this server that StartOS monitors.',
      ),
      default: '0',
      values: Object.fromEntries(
        devices.map((device, index) => [String(index), deviceName(device)]),
      ),
    }),
    ...sharingField(i18n),
  })
}

function clientFields(i18n: i18nPipe) {
  return {
    host: ISB.Value.text({
      name: i18n.transform('NUT server host'),
      description: i18n.transform(
        'The IP address or hostname of the device running the NUT server.',
      ),
      required: true,
      default: null,
      placeholder: '192.168.1.10',
    }),
    port: ISB.Value.number({
      name: i18n.transform('NUT server port'),
      description: i18n.transform(
        'The port the NUT server listens on, usually 3493.',
      ),
      required: true,
      default: 3493,
      integer: true,
      min: 1,
      max: 65535,
    }),
    monitorUsername: ISB.Value.text({
      name: i18n.transform('Username'),
      description: i18n.transform(
        'A user on the NUT server with permission to monitor this UPS.',
      ),
      required: true,
      default: null,
      placeholder: 'monuser',
    }),
    monitorPassword: ISB.Value.text({
      name: i18n.transform('Password'),
      description: i18n.transform(
        'The password for that user on the NUT server.',
      ),
      required: true,
      default: null,
      masked: true,
    }),
  }
}

export function clientSpec(i18n: i18nPipe) {
  return ISB.InputSpec.of(clientFields(i18n))
}

export function clientUpsSpec(
  i18n: i18nPipe,
  upses: readonly T.NutRemoteUps[],
  current?: string,
) {
  return ISB.InputSpec.of({
    upsName: ISB.Value.select({
      name: i18n.transform('UPS'),
      description: i18n.transform(
        'This NUT server monitors more than one UPS. Choose the one that powers this server.',
      ),
      default: upses.some(({ name }) => name === current)
        ? current!
        : upses[0]!.name,
      values: Object.fromEntries(
        upses.map(({ name, description }) => [
          name,
          description ? `${description} (${name})` : name,
        ]),
      ),
    }),
    ...clientFields(i18n),
  })
}

type ServerSettings = Extract<T.NutSettings, { mode: 'server' }>

export function toServerForm({
  listenAll,
  remoteUsername,
  remotePassword,
}: ServerSettings) {
  return {
    sharing: listenAll
      ? { selection: 'on', value: { remoteUsername, remotePassword } }
      : { selection: 'off', value: {} },
  }
}

export function toServerSettings(
  device: T.NutDevice,
  { sharing }: ReturnType<typeof serverSpec>['_TYPE'],
): T.NutSettings {
  return sharing.selection === 'on'
    ? { mode: 'server', device, listenAll: true, ...sharing.value }
    : {
        mode: 'server',
        device,
        listenAll: false,
        remoteUsername: null,
        remotePassword: null,
      }
}

export function toClientSettings(
  upsName: string,
  value: ReturnType<typeof clientSpec>['_TYPE'],
): T.NutSettings {
  return { mode: 'client', upsName, ...value }
}
