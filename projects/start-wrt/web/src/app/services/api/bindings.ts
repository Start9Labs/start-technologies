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

export type ActivityDeleteParamsInput = { id: number }
export type ActivityEntry = {
  id: number
  timestamp: string
  category: string
  action: string
  success: boolean
  summary: string
  error?: string | null
}
export type ActivityListParamsInput = {
  offset?: number | null
  limit?: number | null
}
export type ActivityListResponse = { entries: ActivityEntry[]; total: number }

/**
 * A published port that will be removed because the device it forwards to can
 * no longer be reached at its current address — its security profile is changing
 * (ethernet-port or WiFi-password reassignment) or its WiFi password is being
 * deleted (disconnecting it) — so its DNAT rule, bound to the device's old
 * subnet, would otherwise break. Surfaced to the UI to confirm the deletion.
 */
export type AffectedPublishedPort = {
  id: string
  label: string
  device_mac: string
  device_name: string | null
}
export type Api = {
  _PARAMS: {}
  _CHILDREN: {
    activity: {
      _PARAMS: {}
      _CHILDREN: {
        clear: { _PARAMS: {}; _RETURN: null }
        delete: { _PARAMS: ActivityDeleteParamsInput; _RETURN: null }
        list: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) &
            ActivityListParamsInput
          _RETURN: ActivityListResponse
        }
      }
    }
    auth: {
      _PARAMS: {}
      _CHILDREN: {
        'check-initialized': { _PARAMS: {}; _RETURN: CheckInitializedRes }
        login: { _PARAMS: LoginParamsInput; _RETURN: LoginRes }
        logout: { _PARAMS: LogoutParamsInput; _RETURN: null }
        'set-initial-password': {
          _PARAMS: SetInitialPasswordParamsInput
          _RETURN: LoginRes
        }
        'set-password': { _PARAMS: ResetPasswordParamsInput; _RETURN: null }
        'verify-password': { _PARAMS: VerifyPasswordParamsInput; _RETURN: null }
      }
    }
    backup: {
      _PARAMS: {}
      _CHILDREN: {
        create: { _PARAMS: {}; _RETURN: BackupCreateRes }
        restore: { _PARAMS: {}; _RETURN: BackupRestoreRes }
      }
    }
    devices: {
      _PARAMS: {}
      _CHILDREN: {
        'data-usage': {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) &
            /**
             * A clap-compatible wrapper that reads the inner value as JSON from stdin
             * instead of from CLI args. Used for commands that take complex structured
             * input (entire profile objects, etc.).
             *
             * This is a thin wrapper around `startos::util::serde::StdinDeserializable`
             * that adds a `CommandFactory` impl so it can be used directly as a handler
             * `Params` type (rather than flattened inside a `#[derive(Parser)]` struct,
             * which is how startos uses its version).
             */ DataUsageReqInput
          _RETURN: DataUsagePoint[]
        }
        forget: {
          _PARAMS: /**
           * A clap-compatible wrapper that reads the inner value as JSON from stdin
           * instead of from CLI args. Used for commands that take complex structured
           * input (entire profile objects, etc.).
           *
           * This is a thin wrapper around `startos::util::serde::StdinDeserializable`
           * that adds a `CommandFactory` impl so it can be used directly as a handler
           * `Params` type (rather than flattened inside a `#[derive(Parser)]` struct,
           * which is how startos uses its version).
           */ DeviceMacReqInput
          _RETURN: null
        }
        list: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
          _RETURN: Device[]
        }
        'set-auto-forward': {
          _PARAMS: /**
           * A clap-compatible wrapper that reads the inner value as JSON from stdin
           * instead of from CLI args. Used for commands that take complex structured
           * input (entire profile objects, etc.).
           *
           * This is a thin wrapper around `startos::util::serde::StdinDeserializable`
           * that adds a `CommandFactory` impl so it can be used directly as a handler
           * `Params` type (rather than flattened inside a `#[derive(Parser)]` struct,
           * which is how startos uses its version).
           */ SetAutoForwardRequestInput
          _RETURN: null
        }
        'set-dns-injection': {
          _PARAMS: /**
           * A clap-compatible wrapper that reads the inner value as JSON from stdin
           * instead of from CLI args. Used for commands that take complex structured
           * input (entire profile objects, etc.).
           *
           * This is a thin wrapper around `startos::util::serde::StdinDeserializable`
           * that adds a `CommandFactory` impl so it can be used directly as a handler
           * `Params` type (rather than flattened inside a `#[derive(Parser)]` struct,
           * which is how startos uses its version).
           */ SetDnsInjectionReqInput
          _RETURN: null
        }
        update: {
          _PARAMS: /**
           * A clap-compatible wrapper that reads the inner value as JSON from stdin
           * instead of from CLI args. Used for commands that take complex structured
           * input (entire profile objects, etc.).
           *
           * This is a thin wrapper around `startos::util::serde::StdinDeserializable`
           * that adds a `CommandFactory` impl so it can be used directly as a handler
           * `Params` type (rather than flattened inside a `#[derive(Parser)]` struct,
           * which is how startos uses its version).
           */ DeviceUpdateReqInput
          _RETURN: null
        }
      }
    }
    diagnostics: {
      _PARAMS: {}
      _CHILDREN: { create: { _PARAMS: {}; _RETURN: DiagnosticsCreateRes } }
    }
    dir: {
      _PARAMS: {}
      _CHILDREN: {
        get: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & DirGetArgsInput
          _RETURN: DirEntry[]
        }
      }
    }
    dns: {
      _PARAMS: {}
      _CHILDREN: {
        'injected-list': {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
          _RETURN: InjectedDnsRecord[]
        }
      }
    }
    ethernet: {
      _PARAMS: {}
      _CHILDREN: {
        edit: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
          _RETURN: null
        }
        get: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
          _RETURN: {
            wan_ipv6: boolean
            wan_port: string | null
            ports: { [key: string]: { profile: ProfileId | null } }
          }
        }
        set: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) &
            /**
             * A clap-compatible wrapper that reads the inner value as JSON from stdin
             * instead of from CLI args. Used for commands that take complex structured
             * input (entire profile objects, etc.).
             *
             * This is a thin wrapper around `startos::util::serde::StdinDeserializable`
             * that adds a `CommandFactory` impl so it can be used directly as a handler
             * `Params` type (rather than flattened inside a `#[derive(Parser)]` struct,
             * which is how startos uses its version).
             */ EthernetSetRequestInput
          _RETURN: EthernetSetResult
        }
      }
    }
    exec: {
      _PARAMS: ({ format?: IoFormatInput | null } & {}) & ExecReqInput
      _RETURN: ExecRes
    }
    file: {
      _PARAMS: {}
      _CHILDREN: {
        get: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & GetFileArgsInput
          _RETURN: FileContents
        }
        set: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & SetFileArgsInput
          _RETURN: null
        }
      }
    }
    lan: {
      _PARAMS: {}
      _CHILDREN: {
        'ipv4-get': {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
          _RETURN: LanIpv4Response
        }
        'ipv4-set': {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) &
            /**
             * A clap-compatible wrapper that reads the inner value as JSON from stdin
             * instead of from CLI args. Used for commands that take complex structured
             * input (entire profile objects, etc.).
             *
             * This is a thin wrapper around `startos::util::serde::StdinDeserializable`
             * that adds a `CommandFactory` impl so it can be used directly as a handler
             * `Params` type (rather than flattened inside a `#[derive(Parser)]` struct,
             * which is how startos uses its version).
             */ LanIpv4SetRequestInput
          _RETURN: null
        }
        'ipv6-get': {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
          _RETURN: LanIpv6Response
        }
        'ipv6-set': {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) &
            /**
             * A clap-compatible wrapper that reads the inner value as JSON from stdin
             * instead of from CLI args. Used for commands that take complex structured
             * input (entire profile objects, etc.).
             *
             * This is a thin wrapper around `startos::util::serde::StdinDeserializable`
             * that adds a `CommandFactory` impl so it can be used directly as a handler
             * `Params` type (rather than flattened inside a `#[derive(Parser)]` struct,
             * which is how startos uses its version).
             */ LanIpv6SetRequestInput
          _RETURN: null
        }
      }
    }
    profiles: {
      _PARAMS: {}
      _CHILDREN: {
        create: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) &
            /**
             * A clap-compatible wrapper that reads the inner value as JSON from stdin
             * instead of from CLI args. Used for commands that take complex structured
             * input (entire profile objects, etc.).
             *
             * This is a thin wrapper around `startos::util::serde::StdinDeserializable`
             * that adds a `CommandFactory` impl so it can be used directly as a handler
             * `Params` type (rather than flattened inside a `#[derive(Parser)]` struct,
             * which is how startos uses its version).
             */ ({
              gateway_ip: string
              outbound: string
              lan_access:
                | 'ALL'
                | 'SAME_PROFILE'
                | { other_profiles: ProfileIdOptInput[] }
              wan_access: WanAccessInput
              dns_override?: DnsServerInput[]
              dns_source?: string
              access_to_new_profiles: boolean
              owns_lan: boolean
            } & ProfileIdOptInput)
          _RETURN: ProfileId
        }
        delete: { _PARAMS: ProfileIdOptInput; _RETURN: null }
        edit: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & EditArgsInput
          _RETURN: ProfileId
        }
        get: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & ProfileIdOptInput
          _RETURN: {
            gateway_ip: string
            outbound: string
            lan_access: 'ALL' | 'SAME_PROFILE' | { other_profiles: ProfileId[] }
            wan_access: WanAccess
            dns_override: DnsServer[]
            dns_source: string
            access_to_new_profiles: boolean
            owns_lan: boolean
          } & ProfileId
        }
        list: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
          _RETURN: ProfileId[]
        }
        'schedule-get': {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) &
            /**
             * A clap-compatible wrapper that reads the inner value as JSON from stdin
             * instead of from CLI args. Used for commands that take complex structured
             * input (entire profile objects, etc.).
             *
             * This is a thin wrapper around `startos::util::serde::StdinDeserializable`
             * that adds a `CommandFactory` impl so it can be used directly as a handler
             * `Params` type (rather than flattened inside a `#[derive(Parser)]` struct,
             * which is how startos uses its version).
             */ ScheduleGetParamsInput
          _RETURN: ScheduleWindow[]
        }
        'schedule-set': {
          _PARAMS: /**
           * A clap-compatible wrapper that reads the inner value as JSON from stdin
           * instead of from CLI args. Used for commands that take complex structured
           * input (entire profile objects, etc.).
           *
           * This is a thin wrapper around `startos::util::serde::StdinDeserializable`
           * that adds a `CommandFactory` impl so it can be used directly as a handler
           * `Params` type (rather than flattened inside a `#[derive(Parser)]` struct,
           * which is how startos uses its version).
           */ ScheduleWindowsInput
          _RETURN: null
        }
        set: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) &
            /**
             * A clap-compatible wrapper that reads the inner value as JSON from stdin
             * instead of from CLI args. Used for commands that take complex structured
             * input (entire profile objects, etc.).
             *
             * This is a thin wrapper around `startos::util::serde::StdinDeserializable`
             * that adds a `CommandFactory` impl so it can be used directly as a handler
             * `Params` type (rather than flattened inside a `#[derive(Parser)]` struct,
             * which is how startos uses its version).
             */ ProfileSetRequestInput
          _RETURN: ProfileId
        }
      }
    }
    'published-ports': {
      _PARAMS: {}
      _CHILDREN: {
        'auto-list': {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
          _RETURN: AutomaticPortUse[]
        }
        list: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
          _RETURN: PublishedPort[]
        }
        reconcile: { _PARAMS: {}; _RETURN: unknown }
        set: {
          _PARAMS: /**
           * A clap-compatible wrapper that reads the inner value as JSON from stdin
           * instead of from CLI args. Used for commands that take complex structured
           * input (entire profile objects, etc.).
           *
           * This is a thin wrapper around `startos::util::serde::StdinDeserializable`
           * that adds a `CommandFactory` impl so it can be used directly as a handler
           * `Params` type (rather than flattened inside a `#[derive(Parser)]` struct,
           * which is how startos uses its version).
           */ PublishedPortsSetRequestInput
          _RETURN: PublishedPortsSetResult
        }
        'sync-hairpin': { _PARAMS: {}; _RETURN: unknown }
        'wan-changed': { _PARAMS: {}; _RETURN: unknown }
      }
    }
    setup: {
      _PARAMS: {}
      _CHILDREN: { status: { _PARAMS: {}; _RETURN: SetupStatusRes } }
    }
    'ssh-keys': {
      _PARAMS: {}
      _CHILDREN: {
        add: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) &
            SshKeyAddParamsInput
          _RETURN: SshKeyResponse
        }
        delete: { _PARAMS: SshKeyDeleteParamsInput; _RETURN: null }
        list: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
          _RETURN: SshKeyResponse[]
        }
      }
    }
    system: {
      _PARAMS: {}
      _CHILDREN: {
        'apply-remote-access': { _PARAMS: {}; _RETURN: unknown }
        'factory-reset': { _PARAMS: {}; _RETURN: null }
        'get-timezones': {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
          _RETURN: string[]
        }
        info: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
          _RETURN: SystemInfoResponse
        }
        logs: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
          _RETURN: LogsResponse
        }
        'newer-versions': {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
          _RETURN: VersionInfo[]
        }
        restart: { _PARAMS: {}; _RETURN: null }
        'set-preferences': {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) &
            /**
             * A clap-compatible wrapper that reads the inner value as JSON from stdin
             * instead of from CLI args. Used for commands that take complex structured
             * input (entire profile objects, etc.).
             *
             * This is a thin wrapper around `startos::util::serde::StdinDeserializable`
             * that adds a `CommandFactory` impl so it can be used directly as a handler
             * `Params` type (rather than flattened inside a `#[derive(Parser)]` struct,
             * which is how startos uses its version).
             */ SetPreferencesReqInput
          _RETURN: unknown
        }
        'set-timezone': {
          _PARAMS: /**
           * A clap-compatible wrapper that reads the inner value as JSON from stdin
           * instead of from CLI args. Used for commands that take complex structured
           * input (entire profile objects, etc.).
           *
           * This is a thin wrapper around `startos::util::serde::StdinDeserializable`
           * that adds a `CommandFactory` impl so it can be used directly as a handler
           * `Params` type (rather than flattened inside a `#[derive(Parser)]` struct,
           * which is how startos uses its version).
           */ SetTimezoneParamsInput
          _RETURN: null
        }
        update: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) &
            UpdateSystemParamsInput
          _RETURN: UpdateSystemRes
        }
      }
    }
    uci: {
      _PARAMS: {}
      _CHILDREN: {
        edit: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & GetArgsInput
          _RETURN: { [key: string]: string }
        }
        get: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & GetArgsInput
          _RETURN: { [key: string]: UciFile }
        }
        set: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & /**
           * A clap-compatible wrapper that reads the inner value as JSON from stdin
           * instead of from CLI args. Used for commands that take complex structured
           * input (entire profile objects, etc.).
           *
           * This is a thin wrapper around `startos::util::serde::StdinDeserializable`
           * that adds a `CommandFactory` impl so it can be used directly as a handler
           * `Params` type (rather than flattened inside a `#[derive(Parser)]` struct,
           * which is how startos uses its version).
           */ { [key: string]: UciFileInput }
          _RETURN: { [key: string]: string }
        }
      }
    }
    'vpn-client': {
      _PARAMS: {}
      _CHILDREN: {
        create: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) &
            /**
             * A clap-compatible wrapper that reads the inner value as JSON from stdin
             * instead of from CLI args. Used for commands that take complex structured
             * input (entire profile objects, etc.).
             *
             * This is a thin wrapper around `startos::util::serde::StdinDeserializable`
             * that adds a `CommandFactory` impl so it can be used directly as a handler
             * `Params` type (rather than flattened inside a `#[derive(Parser)]` struct,
             * which is how startos uses its version).
             */ OutboundVpnCreateRequestInput
          _RETURN: OutboundVpnCreateResponse
        }
        delete: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) &
            OutboundVpnDeleteRequestInput
          _RETURN: null
        }
        list: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
          _RETURN: OutboundVpn[]
        }
        'set-enabled': {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) &
            /**
             * A clap-compatible wrapper that reads the inner value as JSON from stdin
             * instead of from CLI args. Used for commands that take complex structured
             * input (entire profile objects, etc.).
             *
             * This is a thin wrapper around `startos::util::serde::StdinDeserializable`
             * that adds a `CommandFactory` impl so it can be used directly as a handler
             * `Params` type (rather than flattened inside a `#[derive(Parser)]` struct,
             * which is how startos uses its version).
             */ OutboundVpnSetEnabledRequestInput
          _RETURN: null
        }
        update: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) &
            /**
             * A clap-compatible wrapper that reads the inner value as JSON from stdin
             * instead of from CLI args. Used for commands that take complex structured
             * input (entire profile objects, etc.).
             *
             * This is a thin wrapper around `startos::util::serde::StdinDeserializable`
             * that adds a `CommandFactory` impl so it can be used directly as a handler
             * `Params` type (rather than flattened inside a `#[derive(Parser)]` struct,
             * which is how startos uses its version).
             */ OutboundVpnUpdateRequestInput
          _RETURN: null
        }
      }
    }
    'vpn-server': {
      _PARAMS: {}
      _CHILDREN: {
        delete: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & DeleteArgsInput
          _RETURN: null
        }
        list: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
          _RETURN: VpnServers
        }
        'peer-add': {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) &
            /**
             * A clap-compatible wrapper that reads the inner value as JSON from stdin
             * instead of from CLI args. Used for commands that take complex structured
             * input (entire profile objects, etc.).
             *
             * This is a thin wrapper around `startos::util::serde::StdinDeserializable`
             * that adds a `CommandFactory` impl so it can be used directly as a handler
             * `Params` type (rather than flattened inside a `#[derive(Parser)]` struct,
             * which is how startos uses its version).
             */ PeerAddArgsInput
          _RETURN: PeerAddResponse
        }
        'peer-delete': {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) &
            PeerDeleteArgsInput
          _RETURN: null
        }
        set: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) &
            /**
             * A clap-compatible wrapper that reads the inner value as JSON from stdin
             * instead of from CLI args. Used for commands that take complex structured
             * input (entire profile objects, etc.).
             *
             * This is a thin wrapper around `startos::util::serde::StdinDeserializable`
             * that adds a `CommandFactory` impl so it can be used directly as a handler
             * `Params` type (rather than flattened inside a `#[derive(Parser)]` struct,
             * which is how startos uses its version).
             */ SetArgsInput
          _RETURN: null
        }
      }
    }
    wan: {
      _PARAMS: {}
      _CHILDREN: {
        'ddns-get': {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
          _RETURN: WanDdnsResponse
        }
        'ddns-set': {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) &
            /**
             * A clap-compatible wrapper that reads the inner value as JSON from stdin
             * instead of from CLI args. Used for commands that take complex structured
             * input (entire profile objects, etc.).
             *
             * This is a thin wrapper around `startos::util::serde::StdinDeserializable`
             * that adds a `CommandFactory` impl so it can be used directly as a handler
             * `Params` type (rather than flattened inside a `#[derive(Parser)]` struct,
             * which is how startos uses its version).
             */ WanDdnsSetRequestInput
          _RETURN: null
        }
        'dns-get': {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
          _RETURN: WanDnsResponse
        }
        'dns-set': {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) &
            /**
             * A clap-compatible wrapper that reads the inner value as JSON from stdin
             * instead of from CLI args. Used for commands that take complex structured
             * input (entire profile objects, etc.).
             *
             * This is a thin wrapper around `startos::util::serde::StdinDeserializable`
             * that adds a `CommandFactory` impl so it can be used directly as a handler
             * `Params` type (rather than flattened inside a `#[derive(Parser)]` struct,
             * which is how startos uses its version).
             */ WanDnsSetRequestInput
          _RETURN: null
        }
        'ipv4-get': {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
          _RETURN: WanIpv4Response
        }
        'ipv4-set': {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) &
            /**
             * A clap-compatible wrapper that reads the inner value as JSON from stdin
             * instead of from CLI args. Used for commands that take complex structured
             * input (entire profile objects, etc.).
             *
             * This is a thin wrapper around `startos::util::serde::StdinDeserializable`
             * that adds a `CommandFactory` impl so it can be used directly as a handler
             * `Params` type (rather than flattened inside a `#[derive(Parser)]` struct,
             * which is how startos uses its version).
             */ WanIpv4SetRequestInput
          _RETURN: null
        }
        'ipv6-get': {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
          _RETURN: WanIpv6Response
        }
        'ipv6-set': {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) &
            /**
             * A clap-compatible wrapper that reads the inner value as JSON from stdin
             * instead of from CLI args. Used for commands that take complex structured
             * input (entire profile objects, etc.).
             *
             * This is a thin wrapper around `startos::util::serde::StdinDeserializable`
             * that adds a `CommandFactory` impl so it can be used directly as a handler
             * `Params` type (rather than flattened inside a `#[derive(Parser)]` struct,
             * which is how startos uses its version).
             */ WanIpv6SetRequestInput
          _RETURN: null
        }
        'mac-get': {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
          _RETURN: WanMacResponse
        }
        'mac-set': {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) &
            /**
             * A clap-compatible wrapper that reads the inner value as JSON from stdin
             * instead of from CLI args. Used for commands that take complex structured
             * input (entire profile objects, etc.).
             *
             * This is a thin wrapper around `startos::util::serde::StdinDeserializable`
             * that adds a `CommandFactory` impl so it can be used directly as a handler
             * `Params` type (rather than flattened inside a `#[derive(Parser)]` struct,
             * which is how startos uses its version).
             */ WanMacSetRequestInput
          _RETURN: null
        }
      }
    }
    wifi: {
      _PARAMS: {}
      _CHILDREN: {
        'blackout-get': {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
          _RETURN: BlackoutWindow[]
        }
        'blackout-set': {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) &
            /**
             * A clap-compatible wrapper that reads the inner value as JSON from stdin
             * instead of from CLI args. Used for commands that take complex structured
             * input (entire profile objects, etc.).
             *
             * This is a thin wrapper around `startos::util::serde::StdinDeserializable`
             * that adds a `CommandFactory` impl so it can be used directly as a handler
             * `Params` type (rather than flattened inside a `#[derive(Parser)]` struct,
             * which is how startos uses its version).
             */ BlackoutWindowsInput
          _RETURN: null
        }
        edit: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
          _RETURN: null
        }
        'generate-password': {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
          _RETURN: string
        }
        get: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
          _RETURN: {
            ssid: string
            broadcastSeparately: boolean
            /**
             * Unset runs the world regulatory domain.
             */
            country: string | null
            radios: { [key: string]: WifiRadio }
            passwords: {
              label: string
              profile: ProfileId | null
              password: string
            }[]
          }
        }
        regulatory: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
          _RETURN: WifiRegulatory
        }
        set: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) &
            /**
             * A clap-compatible wrapper that reads the inner value as JSON from stdin
             * instead of from CLI args. Used for commands that take complex structured
             * input (entire profile objects, etc.).
             *
             * This is a thin wrapper around `startos::util::serde::StdinDeserializable`
             * that adds a `CommandFactory` impl so it can be used directly as a handler
             * `Params` type (rather than flattened inside a `#[derive(Parser)]` struct,
             * which is how startos uses its version).
             */ WifiSetRequestInput
          _RETURN: WifiSetResult
        }
      }
    }
  }
}

/**
 * One automatic port use for the published-ports UI.
 */
export type AutomaticPortUse = {
  /**
   * UCI section name (`apf_<mac>_<extport>`).
   */
  id: string
  /**
   * Which mechanism created it ("PCP", "UPnP", or "SNI").
   */
  kind: string
  device_mac: string
  device_name: string | null
  /**
   * Forward target address on the LAN.
   */
  internal_ip: string | null
  /**
   * Internal port (or range).
   */
  ports: string
  /**
   * External port (or range) on the WAN.
   */
  public_ports: string
  /**
   * Seconds until the lease expires if not renewed (None when the daemon
   * isn't tracking it yet, e.g. right after boot).
   */
  expires_secs: number | null
  /**
   * Hostname for an SNI route; absent for plain forwards.
   */
  hostname: string | null
}
export type BackupCreateRes = { guid: Guid; filename: string }
export type BackupRestoreRes = { upload: Guid }
export type BlackoutWindow = {
  startTime: string
  endTime: string
  days: [boolean, boolean, boolean, boolean, boolean, boolean, boolean]
}
export type BlackoutWindowInput = {
  startTime: string
  endTime: string
  days: [boolean, boolean, boolean, boolean, boolean, boolean, boolean]
}
export type BlackoutWindowsInput = { windows: BlackoutWindowInput[] }
export type CheckInitializedRes = { initialized: boolean }
export type DataUsagePeriodInput = 'week' | 'month' | '3months'
export type DataUsagePoint = {
  timestamp: number
  upload: number
  download: number
}
export type DataUsageReqInput = { mac: string; period: DataUsagePeriodInput }
export type DdnsProvider =
  | 'dyndns'
  | 'noip'
  | 'cloudflare'
  | 'duckdns'
  | 'freedns'
export type DdnsProviderInput =
  | 'dyndns'
  | 'noip'
  | 'cloudflare'
  | 'duckdns'
  | 'freedns'
export type DeleteArgsInput = {
  /**
   * Profile interface name
   */
  profile: string
}
export type Device = {
  mac: string | null
  /**
   * Fully-resolved display name: UCI static name → live DHCP hostname →
   * remembered hostname (cache) → `device-<mac>` placeholder.
   */
  name: string
  /**
   * The UCI static name; `None` when `name` is resolved from elsewhere.
   * What the rename form edits.
   */
  custom_name: string | null
  /**
   * Raw DHCP lease hostname (may be "*").
   */
  hostname: string | null
  status: DeviceStatus
  connection: string | null
  ipv4: string | null
  ipv6: string | null
  ipv4_static: boolean
  /**
   * Whether this device may auto-create port forwards via PCP/UPnP
   * (default off; set via `devices set-auto-forward`).
   */
  allow_auto_port_forward: boolean
  /**
   * Whether this device may publish DNS records into the router's resolver
   * (`devices set-dns-injection`).
   */
  allow_dns_injection: boolean
  security_profile: string | null
  speed: SpeedData | null
  data_usage: (number | null) | null
}
export type DeviceMacReqInput = { mac: string }
export type DeviceStatus = 'online' | 'offline'
export type DeviceUpdateReqInput = {
  mac: string
  /**
   * Absent leaves the assigned name untouched; empty clears it. Otherwise
   * it must pass [`validate_device_name`].
   */
  name?: string | null
  ipv4_static: boolean
  ipv4: string
}
export type DiagnosticsCreateRes = { guid: Guid; filename: string }
export type DirEntry = {
  name: string
  size: number
  blocks: number
  io_block: number
  file_type: FileType
  device: number
  inode: number
  links: number
  mode: number
  uid: number
  gid: number
  access: string
  modify: string
  change: string
}
export type DirGetArgsInput = { path: string }
export type DiskState = {
  /**
   * An eMMC device was found.
   */
  emmcFound: boolean
  /**
   * The eMMC has existing firmware (rootfs partition present).
   */
  hasFirmware: boolean
}
export type DnsMode = 'isp' | 'custom'
export type DnsModeInput = 'isp' | 'custom'

/**
 * A single DNS server entry with protocol info.
 * `ssl: false` = plain UDP on port 53.
 * `ssl: true`  = DNS-over-HTTPS via SmartDNS (`server-https`).
 */
export type DnsServer = { address: string; ssl: boolean }

/**
 * A single DNS server entry with protocol info.
 * `ssl: false` = plain UDP on port 53.
 * `ssl: true`  = DNS-over-HTTPS via SmartDNS (`server-https`).
 */
export type DnsServerInput = { address: string; ssl: boolean }
export type EditArgsInput = { get: ProfileIdOptInput; create: boolean }

/**
 * `ethernet.set` request: the desired port layout plus a confirmation flag.
 * Reassigning a port to a different profile moves its devices to a new subnet,
 * which breaks any published ports forwarding to them. When the flag is false
 * and such ports exist, `set` applies nothing and returns them in
 * `EthernetSetResult` for a confirmation dialog; with the flag true it deletes
 * those published ports as part of the reassignment.
 */
export type EthernetSetRequestInput = {
  confirm_published_port_deletion?: boolean
} & {
  wan_ipv6: boolean
  wan_port?: string | null
  ports: { [key: string]: { profile?: ProfileIdOptInput | null } }
}
export type EthernetSetResult = {
  /**
   * Non-empty (and nothing applied) when published ports would be deleted and
   * the caller hasn't confirmed yet. Empty once the change is applied.
   */
  pending_published_port_deletions: AffectedPublishedPort[]
}
export type ExecReqInput = {
  command: string
  args: string[]
  /**
   * Timeout in milliseconds
   */
  timeout: number
}
export type ExecRes = { stdout: string; stderr: string; exitCode: number }
export type FileContents = { contents: string; modified: string }
export type FileType =
  | 'regular-file'
  | 'directory'
  | 'symlink'
  | 'block-device'
  | 'char-device'
  | 'fifo'
  | 'socket'
export type GetArgsInput = { names: string[] }
export type GetFileArgsInput = { path: string }

/**
 * Unguessable random token, used as a one-time REST endpoint path segment.
 * Wraps `startos::util::new_guid()` (160-bit, base32-encoded, 32 chars).
 */
export type Guid = string
export type InjectedDnsRecord = {
  name: string
  rtype: string
  value: string
  ttl: number
  /**
   * The injecting device's address, when known.
   */
  source: string | null
  /**
   * Owning LAN device MAC (uppercase); absent for a WireGuard peer.
   */
  owner_mac: string | null
  /**
   * Owning inbound-VPN peer public key; absent for a LAN device.
   */
  owner_peer: string | null
  /**
   * Display name of the owning LAN device, when one is known.
   */
  device_name: string | null
  /**
   * Profile interface whose subnet the record was injected from.
   */
  profile: string | null
}
export type IoFormatInput =
  | 'json'
  | 'json-pretty'
  | 'yaml'
  | 'cbor'
  | 'toml'
  | 'toml-pretty'
export type LanIpv4Response = { address: string; netmask: string }
export type LanIpv4SetRequestInput = {
  address: string
  /**
   * When true, forcibly delete VPN peers that would break due to block change.
   */
  force?: boolean
}
export type LanIpv6Response = {
  slaac: boolean
  dhcpv6: boolean
  /**
   * Prefix delegation length, e.g. 64
   */
  prefix: number
  /**
   * Current IPv6 address (if assigned)
   */
  ip6addr: string | null
  /**
   * WAN prefix length (read-only context for the UI)
   */
  wan_prefix: number
}
export type LanIpv6SetRequestInput = {
  slaac: boolean
  dhcpv6: boolean
  prefix: number
}
export type LogEntry = { timestamp: string; message: string }
export type LoginParamsInput = { password: string; userAgent?: string | null }
export type LoginRes = { session: string }
export type LogoutParamsInput = {
  /**
   * Session hash injected by auth middleware
   */
  sessionHash?: string | null
}
export type LogsResponse = { entries: LogEntry[] }
export type MacStrategy = 'router' | 'custom'
export type MacStrategyInput = 'router' | 'custom'
export type OutboundVpn = {
  id: string
  label: string
  target: string
  enabled: boolean
  used_by: string[]
  /**
   * True when the VPN's WireGuard interface has at least one IPv6 address;
   * false means profiles routed through this VPN will have IPv6 disabled
   * (per `outbound_supports_ipv6` in profiles.rs).
   */
  supports_ipv6: boolean
  /**
   * Interface MTU, if explicitly set. `None` means the kernel default
   * (1420 on a clean path) is in effect. Editable via `update`.
   */
  mtu: number | null
  /**
   * The server is named by hostname; only `Internet` is a valid target.
   */
  hostname_endpoint: boolean
}
export type OutboundVpnCreateRequestInput = {
  label: string
  target: string
  /**
   * Raw WireGuard .conf file contents
   */
  config: string
}
export type OutboundVpnCreateResponse = { id: string }
export type OutboundVpnDeleteRequestInput = { id: string }
export type OutboundVpnSetEnabledRequestInput = { id: string; enabled: boolean }
export type OutboundVpnUpdateRequestInput = {
  id: string
  label: string
  target: string
  /**
   * Desired interface MTU. `None` (or absent) restores the default: the
   * chain MTU for a VPN connecting through another, else the kernel's.
   */
  mtu?: number | null
}
export type PeerAddArgsInput = {
  /**
   * Profile interface name (e.g., "lan", "guest")
   */
  profile: string
  /**
   * Peer configuration
   */
  peer: VpnServerPeerInput
}

/**
 * Response from adding a peer, contains client config if keys were generated
 */
export type PeerAddResponse = {
  /**
   * WireGuard client configuration (only present if keys were generated server-side)
   */
  client_config?: string | null
  /**
   * The public key of the peer
   */
  public_key: string
  /**
   * The assigned IP address
   */
  ip: string
}
export type PeerDeleteArgsInput = {
  /**
   * Profile interface name
   */
  profile: string
  /**
   * Public key of the peer to delete
   */
  public_key: string
}
export type ProfileId = {
  fullname: string
  interface: string
  vlan_tag: number
}
export type ProfileIdOptInput = {
  fullname?: string | null
  interface?: string | null
  vlan_tag?: number | null
}
export type ProfileSetRequestInput = { force?: boolean } & ({
  gateway_ip: string
  outbound: string
  lan_access: 'ALL' | 'SAME_PROFILE' | { other_profiles: ProfileIdOptInput[] }
  wan_access: WanAccessInput
  dns_override?: DnsServerInput[]
  dns_source?: string
  access_to_new_profiles: boolean
  owns_lan: boolean
} & ProfileIdOptInput)
export type Protocol = 'tcp' | 'udp' | 'tcp+udp'
export type ProtocolInput = 'tcp' | 'udp' | 'tcp+udp'
export type PublishedPort = {
  id: string
  enabled: boolean
  label: string
  device_mac: string
  ports: string
  protocol: Protocol
  ipv4: boolean
  ipv6: boolean
  ipv4_public_port: string | null
  source: string
  /**
   * Whether WAN collision confirmation remains active.
   */
  override_wan_ports: boolean
  status: PublishedPortStatus
  status_reason: string | null
  device_name: string | null
  device_ipv4: string | null
  device_ipv6: string | null
}
export type PublishedPortInputInput = {
  id: string
  enabled: boolean
  label: string
  device_mac: string
  ports: string
  protocol: ProtocolInput
  ipv4: boolean
  ipv6: boolean
  ipv4_public_port?: string | null
  source: string
  /**
   * Confirms an enabled IPv4 WAN collision.
   */
  override_wan_ports?: boolean
}
export type PublishedPortStatus =
  | 'active'
  | 'partial'
  | 'paused'
  | 'error'
  | 'disabled'
export type PublishedPortsSetRequestInput = { ports: PublishedPortInputInput[] }

/**
 * [`set`] response. A non-empty collision list means nothing was applied —
 * the caller confirms and re-saves; empty means the request was applied.
 */
export type PublishedPortsSetResult = {
  pending_wan_port_collisions: WanPortCollision[]
}
export type ResetPasswordParamsInput = {
  oldPassword: string
  newPassword: string
}
export type ScheduleGetParamsInput = { interface: string }
export type ScheduleWindow = {
  startTime: string
  endTime: string
  days: [boolean, boolean, boolean, boolean, boolean, boolean, boolean]
}
export type ScheduleWindowInput = {
  startTime: string
  endTime: string
  days: [boolean, boolean, boolean, boolean, boolean, boolean, boolean]
}
export type ScheduleWindowsInput = {
  interface: string
  windows: ScheduleWindowInput[]
}
export type Section = {
  ty: string
  name: string | null
  options: { [key: string]: string }
  lists: { [key: string]: string[] }
}
export type SectionInput = {
  ty: string
  name?: string | null
  options: { [key: string]: string }
  lists: { [key: string]: string[] }
}
export type SetArgsInput = {
  /**
   * Profile interface name (e.g., "lan", "guest")
   */
  profile: string
  /**
   * VPN server configuration
   */
  config: VpnServerConfigInput
}
export type SetAutoForwardRequestInput = { mac: string; allow: boolean }
export type SetDnsInjectionReqInput = { mac: string; allow: boolean }
export type SetFileArgsInput = {
  path: string
  contents: string
  modified?: string | null
}
export type SetInitialPasswordParamsInput = { password: string }
export type SetPreferencesReqInput = {
  language?: string | null
  theme?: string | null
  remoteAccess?: string | null
}
export type SetTimezoneParamsInput = {
  /**
   * IANA timezone name, e.g. "America/New_York"
   */
  timezone: string
}
export type SetupStatusRes = { setupMode: boolean; disk: DiskState }
export type SniPortUse = {
  ports: string
  hostnames: string[]
  devices: string[]
}
export type SpeedData = { up: number | null; down: number | null }
export type SshKeyAddParamsInput = { key: string }
export type SshKeyDeleteParamsInput = { fingerprint: string }
export type SshKeyResponse = {
  algorithm: string
  fingerprint: string
  hostname: string
}
export type SystemInfoResponse = {
  version: string
  gitHash: string
  language: string
  date: string
  theme: string
  remoteAccess: string
  timezone: string
}
export type UciFile = { sections: Section[]; modified: string | null }
export type UciFileInput = {
  sections: SectionInput[]
  modified?: string | null
}
export type UpdateSystemParamsInput = {
  registry?: string | null
  targetVersion?: string | null
}
export type UpdateSystemRes = { target: string | null; progress: string | null }
export type VerifyPasswordParamsInput = { password: string }
export type VersionInfo = { version: string; releaseNotes: string }

/**
 * VPN server configuration returned by list (excludes sensitive data)
 */
export type VpnServer = {
  /**
   * Profile interface name this VPN server is associated with (e.g., "lan", "guest")
   */
  profile: string
  /**
   * Human-readable label for the VPN server
   */
  label: string
  /**
   * Whether the VPN server is enabled (listening)
   */
  enabled: boolean
  /**
   * Listen port for WireGuard connections
   */
  listen_port: number
  /**
   * Public endpoint address for inbound client connections (hostname or IP)
   */
  endpoint: string
  /**
   * Server's public key
   */
  public_key: string
  /**
   * Server's VPN address (derived from profile's gateway)
   */
  server_address: string
  /**
   * Connected peers
   */
  peers: VpnServerPeer[]
}

/**
 * VPN server configuration for set/create requests
 */
export type VpnServerConfigInput = {
  /**
   * Human-readable label for the VPN server
   */
  label: string
  /**
   * Whether the VPN server is enabled (listening)
   */
  enabled: boolean
  /**
   * Listen port for WireGuard connections
   */
  listen_port: number
  /**
   * Public endpoint address for inbound client connections (hostname or IP)
   */
  endpoint: string
  /**
   * Private key. If not provided, a new key will be generated.
   */
  private_key?: string | null
}

/**
 * Peer configuration for VPN server
 */
export type VpnServerPeer = {
  /**
   * Human-readable name for the peer
   */
  name: string
  /**
   * Assigned IP address within the profile's subnet (auto-allocated if not specified)
   */
  ip?: string | null
  /**
   * Public key. If not provided when adding a peer, a key pair will be generated.
   */
  public_key?: string | null
  /**
   * Pre-shared key (auto-generated if not provided)
   */
  preshared_key?: string | null
  /**
   * Route all traffic (LAN + WAN) through the tunnel. Default (false/absent) = split tunnel (LAN only).
   */
  route_all?: boolean | null
}

/**
 * Peer configuration for VPN server
 */
export type VpnServerPeerInput = {
  /**
   * Human-readable name for the peer
   */
  name: string
  /**
   * Assigned IP address within the profile's subnet (auto-allocated if not specified)
   */
  ip?: string | null
  /**
   * Public key. If not provided when adding a peer, a key pair will be generated.
   */
  public_key?: string | null
  /**
   * Pre-shared key (auto-generated if not provided)
   */
  preshared_key?: string | null
  /**
   * Route all traffic (LAN + WAN) through the tunnel. Default (false/absent) = split tunnel (LAN only).
   */
  route_all?: boolean | null
}

/**
 * Response containing all VPN servers
 */
export type VpnServers = { servers: VpnServer[] }
export type WanAccess =
  | 'ALL'
  | 'NONE'
  | { whitelist: string[] }
  | { blacklist: string[] }
export type WanAccessInput =
  | 'ALL'
  | 'NONE'
  | { whitelist: string[] }
  | { blacklist: string[] }
export type WanDdnsResponse = {
  enabled: boolean
  provider: DdnsProvider
  hostname: string | null
  username: string | null
  password: string | null
  token: string | null
  zone: string | null
}
export type WanDdnsSetRequestInput = {
  enabled: boolean
  provider: DdnsProviderInput
  hostname?: string | null
  username?: string | null
  password?: string | null
  token?: string | null
  zone?: string | null
}
export type WanDnsResponse = { mode: DnsMode; servers: DnsServer[] }
export type WanDnsSetRequestInput = {
  mode: DnsModeInput
  servers?: DnsServerInput[] | null
}
export type WanIpv4Mode = 'dhcp' | 'static' | 'pppoe'
export type WanIpv4ModeInput = 'dhcp' | 'static' | 'pppoe'
export type WanIpv4Response = {
  mode: WanIpv4Mode
  assigned_ip: string | null
  address: string | null
  netmask: string | null
  gateway: string | null
  username: string | null
  password: string | null
  device: string | null
}
export type WanIpv4SetRequestInput = {
  mode: WanIpv4ModeInput
  address?: string | null
  netmask?: string | null
  gateway?: string | null
  username?: string | null
  password?: string | null
  device?: string | null
}
export type WanIpv6Mode = 'disabled' | 'slaac' | 'dhcpv6' | 'static' | '6rd'
export type WanIpv6ModeInput =
  | 'disabled'
  | 'slaac'
  | 'dhcpv6'
  | 'static'
  | '6rd'
export type WanIpv6Response = {
  mode: WanIpv6Mode
  address: string | null
  prefix: string | null
  gateway: string | null
  ip6prefix: string | null
  ip6prefixlen: string | null
  ip4prefixlen: string | null
  border_relay: string | null
  assigned_ipv6: string | null
  /**
   * Static mode: LAN prefix pool for sub-delegation, e.g. "2001:db8:abcd::/48"
   */
  lan_prefix: string | null
}
export type WanIpv6SetRequestInput = {
  mode: WanIpv6ModeInput
  address?: string | null
  prefix?: string | null
  gateway?: string | null
  ip6prefix?: string | null
  ip6prefixlen?: string | null
  ip4prefixlen?: string | null
  border_relay?: string | null
  /**
   * Static mode: LAN prefix pool for sub-delegation, e.g. "2001:db8:abcd::/48"
   */
  lan_prefix?: string | null
}
export type WanMacResponse = {
  strategy: MacStrategy
  mac: string
  default_mac: string
}
export type WanMacSetRequestInput = {
  strategy: MacStrategyInput
  mac?: string | null
}

/**
 * An enabled IPv4 forward overlapping a router- or SNI-owned WAN port.
 */
export type WanPortCollision = {
  id: string
  label: string
  router_service_ports: string[]
  hostname_route_ports: SniPortUse[]
}
export type WifiRadio = {
  band: string
  channel: string
  enabled: boolean
  broadcast: boolean
}
export type WifiRadioInput = {
  band: string
  channel: string
  enabled: boolean
  broadcast: boolean
}
export type WifiRegulatory = {
  /**
   * ISO 3166-1 alpha-2 codes the regulatory database defines.
   */
  countries: string[]
  /**
   * Channels an access point may use under the current country, by band.
   */
  channels: { [key: string]: number[] }
}

/**
 * `wifi.set` request: the desired WiFi config plus a confirmation flag.
 * When a profile loses its last WiFi password its devices can no longer reach
 * that subnet — disconnected if the password was removed, or moved to another
 * profile if it was reassigned — which breaks any published ports forwarding to
 * them. When the flag is false and such ports exist, `set` applies nothing and
 * returns them in `WifiSetResult` for a confirmation dialog; with the flag true
 * it deletes those published ports as part of the change.
 */
export type WifiSetRequestInput = { confirmPublishedPortDeletion?: boolean } & {
  ssid: string
  broadcastSeparately: boolean
  /**
   * Unset runs the world regulatory domain.
   */
  country?: string | null
  radios: { [key: string]: WifiRadioInput }
  passwords: {
    label: string
    profile?: ProfileIdOptInput | null
    password: string
  }[]
}
export type WifiSetResult = {
  /**
   * Non-empty (and nothing applied) when published ports would be deleted and
   * the caller hasn't confirmed yet. Empty once the change is applied.
   */
  pendingPublishedPortDeletions: AffectedPublishedPort[]
}
