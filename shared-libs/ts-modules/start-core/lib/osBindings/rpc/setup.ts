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

export type Api = {
  _PARAMS: {}
  _CHILDREN: {
    auth: {
      _PARAMS: {}
      _CHILDREN: { session: { _PARAMS: {}; _CHILDREN: {} } }
    }
    backup: {
      _PARAMS: {}
      _CHILDREN: {
        target: {
          _PARAMS: {}
          _CHILDREN: { cifs: { _PARAMS: {}; _CHILDREN: {} } }
        }
      }
    }
    db: { _PARAMS: {}; _CHILDREN: { put: { _PARAMS: {}; _CHILDREN: {} } } }
    diagnostic: {
      _PARAMS: {}
      _CHILDREN: {
        disk: {
          _PARAMS: {}
          _CHILDREN: {
            forget: { _PARAMS: {}; _RETURN: null }
            repair: { _PARAMS: {}; _RETURN: null }
          }
        }
      }
    }
    disk: { _PARAMS: {}; _CHILDREN: { repair: { _PARAMS: {}; _RETURN: null } } }
    'git-info': { _PARAMS: {}; _RETURN: string }
    init: { _PARAMS: {}; _CHILDREN: {} }
    kiosk: { _PARAMS: {}; _CHILDREN: {} }
    net: {
      _PARAMS: {}
      _CHILDREN: {
        acme: { _PARAMS: {}; _CHILDREN: {} }
        dns: {
          _PARAMS: {}
          _CHILDREN: {
            query: {
              _PARAMS: ({ format?: IoFormatInput | null } & {}) &
                QueryDnsParamsInput
              _RETURN: QueryDnsRes
            }
          }
        }
        forward: { _PARAMS: {}; _CHILDREN: {} }
        gateway: { _PARAMS: {}; _CHILDREN: {} }
        ssl: { _PARAMS: {}; _CHILDREN: {} }
        tunnel: { _PARAMS: {}; _CHILDREN: {} }
        vhost: { _PARAMS: {}; _CHILDREN: {} }
      }
    }
    notification: { _PARAMS: {}; _CHILDREN: {} }
    package: {
      _PARAMS: {}
      _CHILDREN: {
        action: { _PARAMS: {}; _CHILDREN: {} }
        backup: { _PARAMS: {}; _CHILDREN: {} }
        host: {
          _PARAMS: RequiresPackageIdInput
          _CHILDREN: {
            address: {
              _PARAMS: RequiresHostIdInput
              _CHILDREN: {
                domain: {
                  _PARAMS: {}
                  _CHILDREN: {
                    private: { _PARAMS: {}; _CHILDREN: {} }
                    public: { _PARAMS: {}; _CHILDREN: {} }
                  }
                }
              }
            }
            binding: { _PARAMS: RequiresHostIdInput; _CHILDREN: {} }
          }
        }
      }
    }
    server: {
      _PARAMS: {}
      _CHILDREN: {
        'device-info': {
          _PARAMS: { format?: IoFormatInput | null } & {}
          _CHILDREN: {}
        }
        experimental: { _PARAMS: {}; _CHILDREN: {} }
        host: {
          _PARAMS: {}
          _CHILDREN: {
            address: {
              _PARAMS: {}
              _CHILDREN: {
                domain: {
                  _PARAMS: {}
                  _CHILDREN: {
                    private: { _PARAMS: {}; _CHILDREN: {} }
                    public: { _PARAMS: {}; _CHILDREN: {} }
                  }
                }
              }
            }
            binding: { _PARAMS: {}; _CHILDREN: {} }
          }
        }
        metrics: {
          _PARAMS: { format?: IoFormatInput | null } & {}
          _CHILDREN: {}
        }
      }
    }
    setup: {
      _PARAMS: {}
      _CHILDREN: {
        attach: { _PARAMS: AttachParamsInput; _RETURN: SetupProgress }
        cifs: {
          _PARAMS: {}
          _CHILDREN: {
            verify: {
              _PARAMS: VerifyCifsParamsInput
              _RETURN: { [key: string]: StartOsRecoveryInfo }
            }
          }
        }
        complete: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
          _RETURN: SetupResult
        }
        disk: {
          _PARAMS: {}
          _CHILDREN: {
            list: {
              _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
              _RETURN: DiskInfo[]
            }
          }
        }
        execute: { _PARAMS: SetupExecuteParamsInput; _RETURN: SetupProgress }
        exit: { _PARAMS: {}; _RETURN: null }
        'get-pubkey': {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
          _RETURN: { [key: string]: unknown }
        }
        'install-os': { _PARAMS: InstallOsParamsInput; _RETURN: SetupInfo }
        logs: {
          _PARAMS: {
            limit?: number | null
            cursor?: string | null
            boot?: number | string | null
            before?: boolean
          } & {}
          _RETURN: LogResponse
          _CHILDREN: { follow: { _PARAMS: {}; _RETURN: LogFollowResponse } }
        }
        restart: { _PARAMS: {}; _RETURN: null }
        'set-keyboard': { _PARAMS: KeyboardOptionsInput; _RETURN: null }
        'set-language': { _PARAMS: SetLanguageParamsInput; _RETURN: null }
        shutdown: { _PARAMS: {}; _RETURN: null }
        status: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
          _RETURN: SetupStatusRes
        }
      }
    }
    ssh: { _PARAMS: {}; _CHILDREN: {} }
    util: { _PARAMS: {}; _CHILDREN: {} }
    wifi: {
      _PARAMS: {}
      _CHILDREN: {
        available: { _PARAMS: {}; _CHILDREN: {} }
        country: { _PARAMS: {}; _CHILDREN: {} }
      }
    }
  }
}
export type AttachParamsInput = {
  password?: EncryptedWireInput | null
  guid: string
  kiosk: boolean
}
export type BackupTargetFSInput =
  | (
      | ({ type: 'disk' } & Exclude<{ logicalname: string }, null>)
      | (null extends { logicalname: string } ? { type: 'disk' } : never)
    )
  | (
      | ({ type: 'cifs' } & Exclude<CifsInput, null>)
      | (null extends CifsInput ? { type: 'cifs' } : never)
    )
export type CifsInput = {
  hostname: string
  path: string
  username: string
  password?: string | null
}
export type DataDriveInput = { stablePath: string; wipe: boolean }
export type DiskInfo = {
  logicalname: string
  stablePath: string
  partitionTable: PartitionTable | null
  vendor: string | null
  model: string | null
  partitions: PartitionInfo[]
  capacity: number
  guid: string | null
  filesystem: string | null
}
export type EncryptedWireInput = { encrypted: unknown }
export type FullProgress = { overall: Progress; phases: NamedProgress[] }
export type Guid = string
export type HostIdInput = string
export type InstallOsParamsInput = {
  osDrive?: string | null
  dataDrive?: DataDriveInput | null
}
export type IoFormatInput =
  | 'json'
  | 'json-pretty'
  | 'yaml'
  | 'cbor'
  | 'toml'
  | 'toml-pretty'
export type KeyboardOptionsInput = {
  layout: string
  keymap?: string | null
  model?: string | null
  variant?: string | null
  options?: string[]
}
export type LogEntry = { timestamp: string; message: string; bootId: string }
export type LogFollowResponse = { startCursor: string | null; guid: Guid }
export type LogResponse = {
  entries: LogEntry[]
  startCursor: string | null
  endCursor: string | null
}
export type NamedProgress = { name: string; progress: Progress }
export type PackageIdInput = string
export type PartitionInfo = {
  logicalname: string
  stablePath: string
  label: string | null
  capacity: number
  used: number | null
  available: number | null
  startOs: { [key: string]: StartOsRecoveryInfo }
  legacyBackup: boolean
  guid: string | null
  filesystem: string | null
}
export type PartitionTable = 'mbr' | 'gpt'
export type Progress =
  | null
  | boolean
  | { done: number; total: number | null; units: ProgressUnits | null }
  | FullProgress
export type ProgressUnits = 'bytes' | 'steps'
export type QueryDnsParamsInput = { fqdn: string }
export type QueryDnsRes = { ipv4: string | null; ipv6: string | null }
export type RequiresHostIdInput = { host: HostIdInput }
export type RequiresPackageIdInput = { package: PackageIdInput }
export type ServerHostname = string
export type SetLanguageParamsInput = { language: string }
export type SetupExecuteParamsInput = {
  guid: string
  password?: EncryptedWireInput | null
  recoverySource?:
    | (
        | ({ type: 'migrate' } & { guid: string })
        | ({ type: 'backup' } & {
            target: BackupTargetFSInput
            password: EncryptedWireInput
            serverId: string
          })
      )
    | null
  kiosk: boolean
  hostname?: string | null
}
export type SetupInfo = {
  guid: string | null
  attach: boolean
  mokEnrolled: boolean
  osDrive: string | null
}
export type SetupProgress = { progress: FullProgress; guid: Guid }
export type SetupResult = {
  hostname: string
  rootCa: string
  needsRestart: boolean
}
export type SetupStatusRes =
  | { status: 'needs-install' }
  | (
      | ({ status: 'incomplete' } & Exclude<SetupInfo, null>)
      | (null extends SetupInfo ? { status: 'incomplete' } : never)
    )
  | (
      | ({ status: 'running' } & Exclude<SetupProgress, null>)
      | (null extends SetupProgress ? { status: 'running' } : never)
    )
  | (
      | ({ status: 'complete' } & Exclude<SetupResult, null>)
      | (null extends SetupResult ? { status: 'complete' } : never)
    )
export type StartOsRecoveryInfo = {
  hostname: ServerHostname
  version: string
  timestamp: string
}
export type VerifyCifsParamsInput = {
  hostname: string
  path: string
  username: string
  password?: EncryptedWireInput | null
}
