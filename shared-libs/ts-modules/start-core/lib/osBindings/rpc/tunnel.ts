export type RpcHandler = {
  _PARAMS: unknown
  _RETURN?: unknown
  _CHILDREN?: { [name: string]: RpcHandler }
}

export type RpcMethod<Root extends RpcHandler> =
  | (Root extends { _RETURN: unknown } ? '' : never)
  | (Root extends { _CHILDREN: infer Children }
      ? {
          [Name in keyof Children & string]: Children[Name] extends RpcHandler
            ? RpcMethod<Children[Name]> extends infer ChildMethod extends string
              ? ChildMethod extends ''
                ? Name
                : `${Name}.${ChildMethod}`
              : never
            : never
        }[keyof Children & string]
      : never)

export type RpcParamType<
  Root extends RpcHandler,
  Method extends string,
> = Method extends ''
  ? Root extends { _RETURN: unknown }
    ? Root['_PARAMS']
    : never
  : Root extends { _CHILDREN: infer Children }
    ? Method extends `${infer Head}.${infer Tail}`
      ? Head extends keyof Children
        ? Children[Head] extends RpcHandler
          ? Root['_PARAMS'] & RpcParamType<Children[Head], Tail>
          : never
        : never
      : Method extends keyof Children
        ? Children[Method] extends RpcHandler
          ? Root['_PARAMS'] & RpcParamType<Children[Method], ''>
          : never
        : never
    : never

export type RpcReturnType<
  Root extends RpcHandler,
  Method extends string,
> = Method extends ''
  ? Root extends { _RETURN: infer Return }
    ? Return
    : never
  : Root extends { _CHILDREN: infer Children }
    ? Method extends `${infer Head}.${infer Tail}`
      ? Head extends keyof Children
        ? Children[Head] extends RpcHandler
          ? RpcReturnType<Children[Head], Tail>
          : never
        : never
      : Method extends keyof Children
        ? Children[Method] extends RpcHandler
          ? RpcReturnType<Children[Method], ''>
          : never
        : never
    : never

export type AddDeviceParamsInput = {
  subnet: string
  name: string
  ip?: string | null
  kind?: WgClientKindInput
}
export type AddDnsRecordParamsInput = {
  name: string
  type: string
  value: string
  ttl?: number | null
}
export type AddKeyParamsInput = { name: string; key: AnyVerifyingKeyInput }
export type AddPinholeParamsInput = {
  gua: string
  externalPort: number
  internalPort?: number | null
  label?: string | null
  count?: number | null
}
export type AddPortForwardParamsInput = {
  externalPort: number
  target: string
  label?: string | null
  sni?: string[]
  count?: number | null
}
export type AddSubnetParamsInput = { name: string }
export type AnyVerifyingKeyInput = string
export type Api = {
  _PARAMS: {}
  _CHILDREN: {
    auth: {
      _PARAMS: {}
      _CHILDREN: {
        key: {
          _PARAMS: {}
          _CHILDREN: {
            add: { _PARAMS: AddKeyParamsInput; _RETURN: null }
            list: {
              _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
              _RETURN: AuthKeys
            }
            remove: { _PARAMS: RemoveKeyParamsInput; _RETURN: null }
          }
        }
        login: { _PARAMS: LoginParamsInput; _RETURN: null }
        logout: {
          _PARAMS: LogoutParamsInput
          _RETURN: HasUnenrolledKeys | null
        }
        session: {
          _PARAMS: {}
          _CHILDREN: {
            kill: { _PARAMS: KillParamsInput; _RETURN: null }
            list: {
              _PARAMS: ({ format?: IoFormatInput | null } & {}) &
                ListParamsInput
              _RETURN: SessionList
            }
          }
        }
        'set-password': { _PARAMS: SetPasswordParamsInput; _RETURN: null }
      }
    }
    db: {
      _PARAMS: {}
      _CHILDREN: {
        apply: { _PARAMS: ApplyWithPathParamsInput; _RETURN: null }
        dump: { _PARAMS: DumpParamsInput; _RETURN: Dump }
        subscribe: { _PARAMS: SubscribeParamsInput; _RETURN: SubscribeRes }
      }
    }
    device: {
      _PARAMS: {}
      _CHILDREN: {
        add: { _PARAMS: AddDeviceParamsInput; _RETURN: null }
        list: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) &
            ListDevicesParamsInput
          _RETURN: WgSubnetConfig
        }
        remove: { _PARAMS: RemoveDeviceParamsInput; _RETURN: null }
        'set-auto-port-forward': {
          _PARAMS: SetAutoPortForwardParamsInput
          _RETURN: null
        }
        'set-dns-injection': {
          _PARAMS: SetDnsInjectionParamsInput
          _RETURN: null
        }
        'set-kind': { _PARAMS: SetDeviceKindParamsInput; _RETURN: null }
        'set-wan': { _PARAMS: SetDeviceWanParamsInput; _RETURN: null }
        'show-config': { _PARAMS: ShowConfigParamsInput; _RETURN: string }
      }
    }
    dns: {
      _PARAMS: {}
      _CHILDREN: {
        add: { _PARAMS: AddDnsRecordParamsInput; _RETURN: null }
        list: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
          _RETURN: DnsRecordEntry[]
        }
        remove: { _PARAMS: RemoveDnsRecordParamsInput; _RETURN: null }
      }
    }
    'http-redirect': {
      _PARAMS: {}
      _CHILDREN: {
        list: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
          _RETURN: HttpRedirectStatus[]
        }
        'set-enabled': {
          _PARAMS: SetHttpRedirectEnabledParamsInput
          _RETURN: null
        }
      }
    }
    pinhole: {
      _PARAMS: {}
      _CHILDREN: {
        add: { _PARAMS: AddPinholeParamsInput; _RETURN: null }
        remove: { _PARAMS: RemovePinholeParamsInput; _RETURN: null }
        'set-enabled': { _PARAMS: SetPinholeEnabledParamsInput; _RETURN: null }
        'update-label': {
          _PARAMS: UpdatePinholeLabelParamsInput
          _RETURN: null
        }
      }
    }
    'port-forward': {
      _PARAMS: {}
      _CHILDREN: {
        add: { _PARAMS: AddPortForwardParamsInput; _RETURN: null }
        remove: { _PARAMS: RemovePortForwardParamsInput; _RETURN: null }
        'set-enabled': {
          _PARAMS: SetPortForwardEnabledParamsInput
          _RETURN: null
        }
        'update-label': {
          _PARAMS: UpdatePortForwardLabelParamsInput
          _RETURN: null
        }
      }
    }
    restart: { _PARAMS: {}; _RETURN: null }
    subnet: {
      _PARAMS: SubnetParamsInput
      _CHILDREN: {
        add: { _PARAMS: AddSubnetParamsInput; _RETURN: null }
        remove: { _PARAMS: {}; _RETURN: null }
        'set-dns': { _PARAMS: SetSubnetDnsParamsInput; _RETURN: null }
        'set-ipv6': { _PARAMS: SetSubnetIpv6ParamsInput; _RETURN: null }
        'set-wan': { _PARAMS: SetSubnetWanParamsInput; _RETURN: null }
      }
    }
    update: {
      _PARAMS: {}
      _CHILDREN: {
        apply: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
          _RETURN: TunnelUpdateResult
        }
        check: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
          _RETURN: TunnelUpdateResult
        }
      }
    }
    web: {
      _PARAMS: {}
      _CHILDREN: {
        disable: { _PARAMS: {}; _RETURN: null }
        enable: { _PARAMS: {}; _RETURN: null }
        'generate-certificate': {
          _PARAMS: GenerateCertParamsInput
          _RETURN: string
        }
        'get-available-ips': {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
          _RETURN: string[]
        }
        'get-certificate': {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
          _RETURN: string | null
        }
        'get-listen': {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
          _RETURN: string | null
        }
        'import-certificate': { _PARAMS: TunnelCertDataInput; _RETURN: null }
        'set-listen': { _PARAMS: SetListenParamsInput; _RETURN: null }
        uninit: { _PARAMS: {}; _RETURN: null }
      }
    }
  }
}
export type ApplyParamsInput = { expr: string }
export type ApplyWithPathParamsInput = {
  path?: string | null
} & ApplyParamsInput
export type AuthKeys = { [key: string]: Session }
export type DnsConfig =
  | { type: 'default' }
  | ({ type: 'device' } & { ip: string })
  | ({ type: 'custom' } & { servers: string[] })
export type DnsModeInput = 'default' | 'device' | 'custom'
export type DnsRecordEntry = {
  name: string
  type: string
  value: string
  ttl: number
  source: string | null
}
export type Dump = { id: number; value: unknown }
export type DumpParamsInput = { pointer?: string | null }
export type GenerateCertParamsInput = { subject: string[] }
export type Guid = string
export type HasUnenrolledKeys = null
export type HttpRedirectStatus = {
  ip: string
  enabled: boolean
  forwarded: boolean
}
export type IoFormatInput =
  | 'json'
  | 'json-pretty'
  | 'yaml'
  | 'cbor'
  | 'toml'
  | 'toml-pretty'
export type KillParamsInput = { ids: string[] }
export type ListDevicesParamsInput = { subnet: string }
export type ListParamsInput = {}
export type LoginParamsInput = {
  password: string
  pubkey: AnyVerifyingKeyInput
  ephemeral?: boolean
}
export type LogoutParamsInput = {}
export type RemoveDeviceParamsInput = { subnet: string; ip: string }
export type RemoveDnsRecordParamsInput = { name: string; type?: string | null }
export type RemoveKeyParamsInput = { key: AnyVerifyingKeyInput }
export type RemovePinholeParamsInput = { gua: string; externalPort: number }
export type RemovePortForwardParamsInput = {
  source: string
  hostname?: string | null
}
export type Session = {
  name: string | null
  loggedIn: string
  lastActive: string
  userAgent: string | null
}
export type SessionList = { current: string | null; sessions: AuthKeys }
export type SetAutoPortForwardParamsInput = {
  subnet: string
  ip: string
  enabled: boolean
}
export type SetDeviceKindParamsInput = {
  subnet: string
  ip: string
  kind: WgClientKindInput
}
export type SetDeviceWanParamsInput = {
  subnet: string
  ip: string
  wanIp?: string | null
}
export type SetDnsInjectionParamsInput = {
  subnet: string
  ip: string
  enabled: boolean
}
export type SetHttpRedirectEnabledParamsInput = { ip: string; enabled: boolean }
export type SetListenParamsInput = { listen: string }
export type SetPasswordParamsInput = { password: string }
export type SetPinholeEnabledParamsInput = {
  gua: string
  externalPort: number
  enabled: boolean
}
export type SetPortForwardEnabledParamsInput = {
  source: string
  enabled: boolean
  hostname?: string | null
}
export type SetSubnetDnsParamsInput = {
  mode: DnsModeInput
  deviceIp?: string | null
  servers: string[]
}
export type SetSubnetIpv6ParamsInput = { prefix?: string | null }
export type SetSubnetWanParamsInput = { wanIp?: string | null }
export type ShowConfigParamsInput = { subnet: string; ip: string }
export type SubnetParamsInput = { subnet: string }
export type SubscribeParamsInput = { pointer?: string | null }
export type SubscribeRes = { dump: Dump; guid: Guid }
export type TunnelCertDataInput = { key: string; cert: string }
export type TunnelUpdateResult = {
  status: string
  installed: string
  candidate: string
}
export type UpdatePinholeLabelParamsInput = {
  gua: string
  externalPort: number
  label?: string | null
}
export type UpdatePortForwardLabelParamsInput = {
  source: string
  label?: string | null
  hostname?: string | null
}
export type WgClientKind = 'client' | 'server'
export type WgClientKindInput = 'client' | 'server'
export type WgConfig = {
  name: string
  key: string
  psk: string
  kind: WgClientKind
  allowDnsInjection: boolean
  allowAutoPortForward: boolean
  wanIp: string | null
}
export type WgSubnetClients = { [key: string]: WgConfig }
export type WgSubnetConfig = {
  name: string
  clients: WgSubnetClients
  dns: DnsConfig
  wanIp: string | null
  ipv6: string | null
}
