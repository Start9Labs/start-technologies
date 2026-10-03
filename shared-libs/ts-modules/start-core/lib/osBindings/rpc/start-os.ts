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

export type AcmeProvider = string
export type AcmeProviderInput = string
export type ActionId = string
export type ActionIdInput = string
export type ActionInput = {
  eventId: Guid
  spec: Record<string, unknown>
  value: Record<string, unknown> | null
}
export type ActionResult =
  | (
      | ({ version: '0' } & Exclude<ActionResultV0, null>)
      | (null extends ActionResultV0 ? { version: '0' } : never)
    )
  | (
      | ({ version: '1' } & Exclude<ActionResultV1, null>)
      | (null extends ActionResultV1 ? { version: '1' } : never)
    )
export type ActionResultMember = {
  name: string
  description: string | null
} & ActionResultValue
export type ActionResultV0 = {
  message: string
  value: string | null
  copyable: boolean
  qr: boolean
}
export type ActionResultV1 = {
  title: string
  message: string | null
  result: ActionResultValue | null
}
export type ActionResultValue =
  | ({ type: 'single' } & {
      value: string
      copyable: boolean | null
      qr: boolean | null
      masked: boolean | null
      launchable: boolean | null
    })
  | ({ type: 'multiline' } & {
      value: string
      copyable: boolean | null
      qr: boolean | null
      masked: boolean | null
      filename: string | null
    })
  | ({ type: 'group' } & { value: ActionResultMember[] })
export type AddAdminParamsInput = { signer: GuidInput }
export type AddAssetParamsInput = {
  version: string
  platform: string
  url: string
  signature: AnySignatureInput
  commitment: Blake3CommitmentInput
}
export type AddCategoryParamsInput = { id: string; name: LocaleStringInput }
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
export type AddMirrorParamsInput = {
  url: string
  commitment: MerkleArchiveCommitmentInput
  signature: AnySignatureInput
}
export type AddPackageParamsInput = {
  urls: string[]
  commitment: MerkleArchiveCommitmentInput
  signature: AnySignatureInput
}
export type AddPackageSignerParamsInput = {
  id: PackageIdInput
  signer: GuidInput
  versions?: string | null
  merge: boolean
}
export type AddPackageToCategoryParamsInput = {
  id: string
  package: PackageIdInput
}
export type AddPassthroughParamsInput = {
  hostname: string
  'listen-port': number
  backend: string
  'public-gateway': GatewayIdInput[]
  'private-ip': string[]
}
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
export type AddPrivateDomainParamsInput = {
  fqdn: string
  gateway: GatewayIdInput
}
export type AddPublicDomainParamsInput = {
  fqdn: string
  acme?: AcmeProviderInput | null
  gateway: GatewayIdInput
  internalPort: number
}
export type AddPublicDomainRes = {
  dns: QueryDnsRes
  port: CheckPortRes
  portV6: CheckPortV6Res | null
  challenge: CheckChallengeRes | null
}
export type AddSslOptions = {
  preferredExternalPort: number
  addXForwardedHeaders: boolean
  alpn: AlpnInfo | null
  upstreamCertValidation: UpstreamCertValidation | null
  auth: ProxyAuth | null
}
export type AddSubnetParamsInput = { name: string }
export type AddTunnelParamsInput = {
  name: string
  config: string
  type?: GatewayTypeInput | null
  setAsDefaultOutbound: boolean
}
export type AddVersionParamsInput = {
  version: string
  headline: string
  releaseNotes: string
  sourceVersion: string
}
export type AddressInfo = {
  username: string | null
  hostId: HostId
  internalPort: number
  scheme: string | null
  sslScheme: string | null
  suffix: string
}
export type AlpnInfo = MaybeUtf8String[]
export type AnySignature = string
export type AnySignatureInput = string
export type AnyVerifyingKey = string
export type AnyVerifyingKeyInput = string
export type Api = {
  _PARAMS: {}
  _CHILDREN: {
    auth: {
      _PARAMS: {}
      _CHILDREN: {
        'get-pubkey': { _PARAMS: {}; _RETURN: { [key: string]: unknown } }
        login: { _PARAMS: LoginParamsInput; _RETURN: null }
        logout: {
          _PARAMS: LogoutParamsInput
          _RETURN: HasUnenrolledKeys | null
        }
        'reset-password': { _PARAMS: ResetPasswordParamsInput; _RETURN: null }
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
      }
    }
    backup: {
      _PARAMS: {}
      _CHILDREN: {
        create: { _PARAMS: BackupParamsInput; _RETURN: null }
        target: {
          _PARAMS: {}
          _CHILDREN: {
            cifs: {
              _PARAMS: {}
              _CHILDREN: {
                add: {
                  _PARAMS: CifsAddParamsInput
                  _RETURN: { [key: string]: BackupTarget }
                }
                remove: { _PARAMS: CifsRemoveParamsInput; _RETURN: null }
                update: {
                  _PARAMS: CifsUpdateParamsInput
                  _RETURN: { [key: string]: BackupTarget }
                }
              }
            }
            'delete-legacy': { _PARAMS: DeleteLegacyParamsInput; _RETURN: null }
            info: {
              _PARAMS: ({ format?: IoFormatInput | null } & {}) &
                InfoParamsInput
              _RETURN: BackupInfo
            }
            list: {
              _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
              _RETURN: { [key: string]: BackupTarget }
            }
            mount: { _PARAMS: MountParamsInput; _RETURN: string }
            umount: { _PARAMS: UmountParamsInput; _RETURN: null }
          }
        }
      }
    }
    db: {
      _PARAMS: {}
      _CHILDREN: {
        apply: { _PARAMS: ApplyParamsInput; _RETURN: null }
        dump: { _PARAMS: DumpParamsInput; _RETURN: Dump }
        put: {
          _PARAMS: {}
          _CHILDREN: {
            ui: {
              _PARAMS: ({ format?: IoFormatInput | null } & {}) & UiParamsInput
              _RETURN: null
            }
          }
        }
        subscribe: { _PARAMS: SubscribeParamsInput; _RETURN: SubscribeRes }
      }
    }
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
    disk: {
      _PARAMS: {}
      _CHILDREN: {
        list: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
          _RETURN: DiskInfo[]
        }
        repair: { _PARAMS: {}; _RETURN: null }
      }
    }
    echo: { _PARAMS: EchoParamsInput; _RETURN: string }
    'git-info': { _PARAMS: {}; _RETURN: string }
    init: { _PARAMS: {}; _CHILDREN: {} }
    kiosk: {
      _PARAMS: {}
      _CHILDREN: {
        disable: { _PARAMS: {}; _RETURN: null }
        enable: { _PARAMS: {}; _RETURN: null }
      }
    }
    net: {
      _PARAMS: {}
      _CHILDREN: {
        acme: {
          _PARAMS: {}
          _CHILDREN: {
            'check-challenge': {
              _PARAMS: ({ format?: IoFormatInput | null } & {}) &
                CheckChallengeParamsInput
              _RETURN: CheckChallengeRes | null
            }
            init: { _PARAMS: InitAcmeParamsInput; _RETURN: null }
            remove: { _PARAMS: RemoveAcmeParamsInput; _RETURN: null }
          }
        }
        dns: {
          _PARAMS: {}
          _CHILDREN: {
            'dump-table': {
              _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
              _RETURN: { [key: string]: string | null }
            }
            query: {
              _PARAMS: ({ format?: IoFormatInput | null } & {}) &
                QueryDnsParamsInput
              _RETURN: QueryDnsRes
            }
            'set-static': { _PARAMS: SetStaticDnsParamsInput; _RETURN: null }
          }
        }
        forward: {
          _PARAMS: {}
          _CHILDREN: {
            'dump-table': {
              _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
              _RETURN: ForwardTable
            }
          }
        }
        gateway: {
          _PARAMS: {}
          _CHILDREN: {
            'check-dns': {
              _PARAMS: ({ format?: IoFormatInput | null } & {}) &
                CheckDnsParamsInput
              _RETURN: boolean
            }
            'check-port': {
              _PARAMS: ({ format?: IoFormatInput | null } & {}) &
                CheckPortParamsInput
              _RETURN: CheckPortRes
            }
            'check-port-v6': {
              _PARAMS: ({ format?: IoFormatInput | null } & {}) &
                CheckPortParamsInput
              _RETURN: CheckPortV6Res | null
            }
            forget: { _PARAMS: ForgetGatewayParamsInput; _RETURN: null }
            list: {
              _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
              _RETURN: { [key: string]: NetworkInterfaceInfo }
            }
            'set-default-outbound': {
              _PARAMS: SetDefaultOutboundParamsInput
              _RETURN: null
            }
            'set-name': { _PARAMS: RenameGatewayParamsInput; _RETURN: null }
            'set-secure': {
              _PARAMS: SetGatewaySecureParamsInput
              _RETURN: null
            }
            'unset-secure': {
              _PARAMS: UnsetGatewaySecureParamsInput
              _RETURN: null
            }
          }
        }
        ssl: {
          _PARAMS: {}
          _CHILDREN: {
            'generate-certificate': {
              _PARAMS: ({ format?: IoFormatInput | null } & {}) &
                GenerateCertificateParamsInput
              _RETURN: GenerateCertificateResponse
            }
          }
        }
        tunnel: {
          _PARAMS: {}
          _CHILDREN: {
            add: { _PARAMS: AddTunnelParamsInput; _RETURN: GatewayId }
            remove: { _PARAMS: RemoveTunnelParamsInput; _RETURN: null }
            update: { _PARAMS: UpdateTunnelParamsInput; _RETURN: null }
          }
        }
        vhost: {
          _PARAMS: {}
          _CHILDREN: {
            'add-passthrough': {
              _PARAMS: AddPassthroughParamsInput
              _RETURN: null
            }
            'dump-table': {
              _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
              _RETURN: { [key: string]: { [key: string]: string[] } }
            }
            'list-passthrough': {
              _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
              _RETURN: PassthroughInfo[]
            }
            'remove-passthrough': {
              _PARAMS: RemovePassthroughParamsInput
              _RETURN: null
            }
          }
        }
      }
    }
    notification: {
      _PARAMS: {}
      _CHILDREN: {
        create: { _PARAMS: CreateParamsInput; _RETURN: null }
        list: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) &
            ListNotificationParamsInput
          _RETURN: NotificationWithId[]
        }
        'mark-seen': { _PARAMS: ModifyNotificationParamsInput; _RETURN: null }
        'mark-seen-before': {
          _PARAMS: ModifyNotificationBeforeParamsInput
          _RETURN: null
        }
        'mark-unseen': { _PARAMS: ModifyNotificationParamsInput; _RETURN: null }
        remove: { _PARAMS: ModifyNotificationParamsInput; _RETURN: null }
        'remove-before': {
          _PARAMS: ModifyNotificationBeforeParamsInput
          _RETURN: null
        }
      }
    }
    package: {
      _PARAMS: {}
      _CHILDREN: {
        action: {
          _PARAMS: {}
          _CHILDREN: {
            'clear-task': { _PARAMS: ClearTaskParamsInput; _RETURN: null }
            'get-input': {
              _PARAMS: ({ format?: IoFormatInput | null } & {}) &
                GetActionInputParamsInput
              _RETURN: ActionInput | null
            }
            run: {
              _PARAMS: ({ format?: IoFormatInput | null } & {}) &
                RunActionParamsInput
              _RETURN: ActionResult | null
            }
          }
        }
        attach: { _PARAMS: AttachParamsInput; _RETURN: Guid }
        backup: {
          _PARAMS: {}
          _CHILDREN: {
            restore: { _PARAMS: RestorePackageParamsInput; _RETURN: null }
          }
        }
        'cancel-install': { _PARAMS: CancelInstallParamsInput; _RETURN: null }
        host: {
          _PARAMS: RequiresPackageIdInput
          _CHILDREN: {
            address: {
              _PARAMS: RequiresHostIdInput
              _CHILDREN: {
                domain: {
                  _PARAMS: {}
                  _CHILDREN: {
                    private: {
                      _PARAMS: {}
                      _CHILDREN: {
                        add: {
                          _PARAMS: AddPrivateDomainParamsInput
                          _RETURN: boolean
                        }
                        remove: {
                          _PARAMS: RemoveDomainParamsInput
                          _RETURN: null
                        }
                      }
                    }
                    public: {
                      _PARAMS: {}
                      _CHILDREN: {
                        add: {
                          _PARAMS: AddPublicDomainParamsInput
                          _RETURN: AddPublicDomainRes
                        }
                        remove: {
                          _PARAMS: RemoveDomainParamsInput
                          _RETURN: null
                        }
                      }
                    }
                  }
                }
                list: {
                  _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
                  _RETURN: HostAddress[]
                }
              }
            }
            binding: {
              _PARAMS: RequiresHostIdInput
              _CHILDREN: {
                list: {
                  _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
                  _RETURN: Bindings
                }
                'set-address-enabled': {
                  _PARAMS: BindingSetAddressEnabledParamsInput
                  _RETURN: null
                }
                'set-gua-wan': {
                  _PARAMS: BindingSetGuaWanParamsInput
                  _RETURN: null
                }
                'set-range-address-enabled': {
                  _PARAMS: BindingSetAddressEnabledParamsInput
                  _RETURN: null
                }
              }
            }
            list: { _PARAMS: {}; _RETURN: HostId[] }
          }
        }
        install: { _PARAMS: InstallParamsInput; _RETURN: null }
        'installed-version': {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) &
            InstalledVersionParamsInput
          _RETURN: Version | null
        }
        list: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
          _RETURN: unknown[]
        }
        logs: {
          _PARAMS: {
            limit?: number | null
            cursor?: string | null
            boot?: number | string | null
            before?: boolean
          } & PackageIdParamsInput
          _RETURN: LogResponse
          _CHILDREN: { follow: { _PARAMS: {}; _RETURN: LogFollowResponse } }
        }
        rebuild: { _PARAMS: RebuildParamsInput; _RETURN: null }
        restart: { _PARAMS: ControlParamsInput; _RETURN: null }
        'set-outbound-gateway': {
          _PARAMS: SetOutboundGatewayParamsInput
          _RETURN: null
        }
        sideload: { _PARAMS: SideloadParamsInput; _RETURN: SideloadResponse }
        start: { _PARAMS: StartParamsInput; _RETURN: null }
        stats: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
          _RETURN: { [key: string]: ServiceStats | null }
        }
        stop: { _PARAMS: ControlParamsInput; _RETURN: null }
        uninstall: { _PARAMS: UninstallParamsInput; _RETURN: null }
      }
    }
    registry: {
      _PARAMS: {} & RegistryUrlParamsInput
      _CHILDREN: {
        admin: {
          _PARAMS: {}
          _CHILDREN: {
            add: { _PARAMS: AddAdminParamsInput; _RETURN: null }
            list: {
              _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
              _RETURN: { [key: string]: SignerInfo }
            }
            remove: { _PARAMS: RemoveAdminParamsInput; _RETURN: null }
            signer: {
              _PARAMS: {}
              _CHILDREN: {
                add: { _PARAMS: SignerInfoInput; _RETURN: Guid }
                edit: { _PARAMS: EditSignerParamsInput; _RETURN: null }
                list: {
                  _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
                  _RETURN: { [key: string]: SignerInfo }
                }
                remove: { _PARAMS: RemoveSignerParamsInput; _RETURN: null }
              }
            }
          }
        }
        db: {
          _PARAMS: {}
          _CHILDREN: {
            apply: { _PARAMS: ApplyWithPathParamsInput; _RETURN: null }
            dump: { _PARAMS: DumpParamsInput; _RETURN: Dump }
            subscribe: { _PARAMS: DumpParamsInput; _RETURN: SubscribeRes }
          }
        }
        index: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
          _RETURN: FullIndex
        }
        info: {
          _PARAMS: { format?: IoFormatInput | null } & {}
          _RETURN: RegistryInfo
          _CHILDREN: {
            'set-description': {
              _PARAMS: SetDescriptionParamsInput
              _RETURN: null
            }
            'set-icon': { _PARAMS: SetIconParamsInput; _RETURN: null }
            'set-name': { _PARAMS: SetNameParamsInput; _RETURN: null }
          }
        }
        metrics: {
          _PARAMS: {}
          _CHILDREN: {
            downloads: {
              _PARAMS: ({ format?: IoFormatInput | null } & {}) &
                GetDownloadsParamsInput
              _RETURN: DownloadsResponse
            }
            summary: {
              _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
              _RETURN: MetricsSummary
            }
            users: {
              _PARAMS: ({ format?: IoFormatInput | null } & {}) &
                GetUsersParamsInput
              _RETURN: UsersResponse
            }
          }
        }
        os: {
          _PARAMS: {}
          _CHILDREN: {
            asset: {
              _PARAMS: {}
              _CHILDREN: {
                add: {
                  _PARAMS: {}
                  _CHILDREN: {
                    img: { _PARAMS: AddAssetParamsInput; _RETURN: null }
                    iso: { _PARAMS: AddAssetParamsInput; _RETURN: null }
                    squashfs: { _PARAMS: AddAssetParamsInput; _RETURN: null }
                  }
                }
                get: {
                  _PARAMS: {}
                  _CHILDREN: {
                    img: {
                      _PARAMS: GetOsAssetParamsInput
                      _RETURN: {
                        publishedAt: string
                        urls: string[]
                        commitment: Blake3Commitment
                        signatures: { [key: string]: AnySignature }
                      }
                    }
                    iso: {
                      _PARAMS: GetOsAssetParamsInput
                      _RETURN: {
                        publishedAt: string
                        urls: string[]
                        commitment: Blake3Commitment
                        signatures: { [key: string]: AnySignature }
                      }
                    }
                    squashfs: {
                      _PARAMS: GetOsAssetParamsInput
                      _RETURN: {
                        publishedAt: string
                        urls: string[]
                        commitment: Blake3Commitment
                        signatures: { [key: string]: AnySignature }
                      }
                    }
                  }
                }
                remove: {
                  _PARAMS: {}
                  _CHILDREN: {
                    img: { _PARAMS: RemoveAssetParamsInput; _RETURN: boolean }
                    iso: { _PARAMS: RemoveAssetParamsInput; _RETURN: boolean }
                    squashfs: {
                      _PARAMS: RemoveAssetParamsInput
                      _RETURN: boolean
                    }
                  }
                }
                sign: {
                  _PARAMS: {}
                  _CHILDREN: {
                    img: { _PARAMS: SignAssetParamsInput; _RETURN: null }
                    iso: { _PARAMS: SignAssetParamsInput; _RETURN: null }
                    squashfs: { _PARAMS: SignAssetParamsInput; _RETURN: null }
                  }
                }
              }
            }
            index: {
              _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
              _RETURN: OsIndex
            }
            version: {
              _PARAMS: {}
              _CHILDREN: {
                add: { _PARAMS: AddVersionParamsInput; _RETURN: null }
                get: {
                  _PARAMS: ({ format?: IoFormatInput | null } & {}) &
                    GetOsVersionParamsInput
                  _RETURN: OsVersionInfoMap
                }
                remove: { _PARAMS: RemoveVersionParamsInput; _RETURN: null }
                signer: {
                  _PARAMS: {}
                  _CHILDREN: {
                    add: { _PARAMS: VersionSignerParamsInput; _RETURN: null }
                    list: {
                      _PARAMS: ({ format?: IoFormatInput | null } & {}) &
                        ListVersionSignersParamsInput
                      _RETURN: { [key: string]: SignerInfo }
                    }
                    remove: { _PARAMS: VersionSignerParamsInput; _RETURN: null }
                  }
                }
              }
            }
          }
        }
        package: {
          _PARAMS: {}
          _CHILDREN: {
            add: { _PARAMS: AddPackageParamsInput; _RETURN: null }
            'add-mirror': { _PARAMS: AddMirrorParamsInput; _RETURN: null }
            category: {
              _PARAMS: {}
              _CHILDREN: {
                add: { _PARAMS: AddCategoryParamsInput; _RETURN: null }
                'add-package': {
                  _PARAMS: AddPackageToCategoryParamsInput
                  _RETURN: null
                }
                list: {
                  _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
                  _RETURN: { [key: string]: Category }
                }
                remove: { _PARAMS: RemoveCategoryParamsInput; _RETURN: null }
                'remove-package': {
                  _PARAMS: RemovePackageFromCategoryParamsInput
                  _RETURN: null
                }
              }
            }
            get: {
              _PARAMS: ({ format?: IoFormatInput | null } & {}) &
                GetPackageParamsInput
              _RETURN: GetPackageResult
            }
            index: {
              _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
              _RETURN: PackageIndex
            }
            remove: { _PARAMS: RemovePackageParamsInput; _RETURN: boolean }
            'remove-mirror': { _PARAMS: RemoveMirrorParamsInput; _RETURN: null }
            signer: {
              _PARAMS: {}
              _CHILDREN: {
                add: { _PARAMS: AddPackageSignerParamsInput; _RETURN: null }
                list: {
                  _PARAMS: ({ format?: IoFormatInput | null } & {}) &
                    ListPackageSignersParamsInput
                  _RETURN: { [key: string]: [SignerInfo, string] }
                }
                remove: {
                  _PARAMS: RemovePackageSignerParamsInput
                  _RETURN: null
                }
              }
            }
          }
        }
      }
    }
    server: {
      _PARAMS: {}
      _CHILDREN: {
        'clear-smtp': { _PARAMS: {}; _RETURN: null }
        'device-info': {
          _PARAMS: { format?: IoFormatInput | null } & {}
          _RETURN: DeviceInfo
          _CHILDREN: {}
        }
        epp: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & EppParamsInput
          _RETURN: EppInfo
        }
        experimental: {
          _PARAMS: {}
          _CHILDREN: { zram: { _PARAMS: ZramParamsInput; _RETURN: null } }
        }
        governor: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) &
            GovernorParamsInput
          _RETURN: GovernorInfo
        }
        host: {
          _PARAMS: {}
          _CHILDREN: {
            address: {
              _PARAMS: {}
              _CHILDREN: {
                domain: {
                  _PARAMS: {}
                  _CHILDREN: {
                    private: {
                      _PARAMS: {}
                      _CHILDREN: {
                        add: {
                          _PARAMS: AddPrivateDomainParamsInput
                          _RETURN: boolean
                        }
                        remove: {
                          _PARAMS: RemoveDomainParamsInput
                          _RETURN: null
                        }
                      }
                    }
                    public: {
                      _PARAMS: {}
                      _CHILDREN: {
                        add: {
                          _PARAMS: AddPublicDomainParamsInput
                          _RETURN: AddPublicDomainRes
                        }
                        remove: {
                          _PARAMS: RemoveDomainParamsInput
                          _RETURN: null
                        }
                      }
                    }
                  }
                }
                list: {
                  _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
                  _RETURN: HostAddress[]
                }
              }
            }
            binding: {
              _PARAMS: {}
              _CHILDREN: {
                list: {
                  _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
                  _RETURN: Bindings
                }
                'set-address-enabled': {
                  _PARAMS: BindingSetAddressEnabledParamsInput
                  _RETURN: null
                }
                'set-gua-wan': {
                  _PARAMS: BindingSetGuaWanParamsInput
                  _RETURN: null
                }
                'set-range-address-enabled': {
                  _PARAMS: BindingSetAddressEnabledParamsInput
                  _RETURN: null
                }
              }
            }
          }
        }
        'kernel-logs': {
          _PARAMS: {
            limit?: number | null
            cursor?: string | null
            boot?: number | string | null
            before?: boolean
          } & {}
          _RETURN: LogResponse
          _CHILDREN: { follow: { _PARAMS: {}; _RETURN: LogFollowResponse } }
        }
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
        metrics: {
          _PARAMS: { format?: IoFormatInput | null } & {}
          _RETURN: Metrics
          _CHILDREN: {
            follow: {
              _PARAMS: MetricsFollowParamsInput
              _RETURN: MetricsFollowResponse
            }
          }
        }
        rebuild: { _PARAMS: {}; _RETURN: null }
        restart: { _PARAMS: ShutdownParamsInput; _RETURN: null }
        'set-echoip-urls': { _PARAMS: SetEchoipUrlsParamsInput; _RETURN: null }
        'set-hostname': { _PARAMS: SetServerHostnameParamsInput; _RETURN: null }
        'set-keyboard': { _PARAMS: KeyboardOptionsInput; _RETURN: null }
        'set-language': { _PARAMS: SetLanguageParamsInput; _RETURN: null }
        'set-smtp': { _PARAMS: SmtpValueInput; _RETURN: null }
        shutdown: { _PARAMS: ShutdownParamsInput; _RETURN: null }
        'test-smtp': { _PARAMS: TestSmtpParamsInput; _RETURN: null }
        time: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
          _RETURN: TimeInfo
        }
        'trust-ca': {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & TrustCaParamsInput
          _RETURN: TrustedCa
        }
        update: { _PARAMS: UpdateSystemParamsInput; _RETURN: UpdateSystemRes }
        'update-firmware': { _PARAMS: {}; _RETURN: RequiresReboot }
      }
    }
    setup: {
      _PARAMS: {}
      _CHILDREN: {
        cifs: { _PARAMS: {}; _CHILDREN: {} }
        disk: { _PARAMS: {}; _CHILDREN: {} }
      }
    }
    ssh: {
      _PARAMS: {}
      _CHILDREN: {
        add: { _PARAMS: SshAddParamsInput; _RETURN: SshKeyResponse }
        list: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
          _RETURN: SshKeyResponse[]
        }
        remove: { _PARAMS: SshDeleteParamsInput; _RETURN: null }
      }
    }
    state: { _PARAMS: {}; _RETURN: ApiState }
    tunnel: {
      _PARAMS: {} & TunnelUrlParamsInput
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
            'set-enabled': {
              _PARAMS: SetPinholeEnabledParamsInput
              _RETURN: null
            }
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
            'import-certificate': {
              _PARAMS: TunnelCertDataInput
              _RETURN: null
            }
            'set-listen': { _PARAMS: SetListenParamsInput; _RETURN: null }
            uninit: { _PARAMS: {}; _RETURN: null }
          }
        }
      }
    }
    util: { _PARAMS: {}; _CHILDREN: {} }
    wifi: {
      _PARAMS: {}
      _CHILDREN: {
        add: { _PARAMS: WifiAddParamsInput; _RETURN: null }
        available: {
          _PARAMS: {}
          _CHILDREN: {
            get: {
              _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
              _RETURN: WifiListOut[]
            }
          }
        }
        connect: { _PARAMS: WifiSsidParamsInput; _RETURN: null }
        country: {
          _PARAMS: {}
          _CHILDREN: { set: { _PARAMS: SetCountryParamsInput; _RETURN: null } }
        }
        get: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
          _RETURN: WifiListInfo
        }
        remove: { _PARAMS: WifiSsidParamsInput; _RETURN: null }
        'set-enabled': { _PARAMS: SetWifiEnabledParamsInput; _RETURN: null }
      }
    }
  }
}
export type ApiState = 'error' | 'initializing' | 'running'
export type ApplyParamsInput = { expr: string }
export type ApplyWithPathParamsInput = {
  path?: string | null
} & ApplyParamsInput
export type AttachParamsInput = {
  id: PackageIdInput
  command: string[]
  tty: boolean
  stderrTty: boolean
  ptySize?: TermSizeInput | null
  subcontainer?: string | null
  name?: string | null
  imageId?: string | null
  user?: string | null
}
export type AuthKeys = { [key: string]: Session }
export type BackupInfo = {
  version: string
  timestamp: string | null
  packageBackups: { [key: string]: PackageBackupInfo }
}
export type BackupParamsInput = {
  targetId: BackupTargetIdInput
  oldPassword?: PasswordTypeInput | null
  packageIds?: PackageIdInput[] | null
  password: PasswordTypeInput
}
export type BackupTarget =
  | ({ type: 'disk' } & ({
      vendor: string | null
      model: string | null
    } & PartitionInfo))
  | (
      | ({ type: 'cifs' } & Exclude<CifsBackupTarget, null>)
      | (null extends CifsBackupTarget ? { type: 'cifs' } : never)
    )
export type BackupTargetIdInput = string
export type BasicCredential = { username: string; password: string }
export type BindInfo = {
  enabled: boolean
  options: BindOptions
  net: NetInfo
  addresses: DerivedAddressInfo
  interfaces: { [key: string]: ServiceInterface }
}
export type BindOptions = {
  preferredExternalPort: number
  addSsl: AddSslOptions | null
  secure: Security | null
}
export type BindingSetAddressEnabledParamsInput = {
  internalPort: number
  address: HostnameInfoInput
  enabled?: boolean | null
}
export type BindingSetGuaWanParamsInput = {
  internalPort: number
  address: HostnameInfoInput
  wan: boolean
}
export type Bindings = { [key: string]: BindInfo }
export type Blake3Commitment = { hash: string; size: number }
export type Blake3CommitmentInput = { hash: string; size: number }
export type CancelInstallParamsInput = { id: PackageIdInput }
export type CapabilityVerdict = { supported: boolean | null; at: string | null }
export type Category = { name: LocaleString }
export type Celsius = { value: string; unit: string }
export type CheckChallengeParamsInput = {
  fqdn: string
  gateway: GatewayIdInput
  acme: AcmeProviderInput
}
export type CheckChallengeRes = {
  port: CheckPortRes | null
  portV6: CheckPortV6Res | null
}
export type CheckDnsParamsInput = { gateway: GatewayIdInput; fqdn: string }
export type CheckPortParamsInput = { port: number; gateway: GatewayIdInput }
export type CheckPortRes = {
  ip: string
  port: number
  openExternally: boolean
  openInternally: boolean
  hairpinning: boolean
}
export type CheckPortV6Res = {
  ip: string
  openExternally: boolean
  openInternally: boolean
}
export type CifsAddParamsInput = {
  hostname: string
  path: string
  username: string
  password?: string | null
}
export type CifsBackupTarget = {
  hostname: string
  path: string
  username: string
  mountable: boolean
  available: number | null
  startOs: { [key: string]: StartOsRecoveryInfo }
  legacyBackup: boolean
}
export type CifsRemoveParamsInput = { id: BackupTargetIdInput }
export type CifsUpdateParamsInput = {
  id: BackupTargetIdInput
  hostname: string
  path: string
  username: string
  password?: string | null
}
export type ClearTaskParamsInput = {
  packageId: PackageIdInput
  replayId: ReplayIdInput
  force?: boolean
}
export type ContactInfo =
  | { email: string }
  | { matrix: string }
  | { website: string }
export type ContactInfoInput =
  | { email: string }
  | { matrix: string }
  | { website: string }
export type ContainerId = string
export type ControlParamsInput = { id: PackageIdInput }
export type CountEntry = { label: string; count: number }
export type CreateParamsInput = {
  package?: PackageIdInput | null
  level: NotificationLevelInput
  title: string
  message: string
}
export type CurrentDependencyKind =
  | { kind: 'exists' }
  | ({ kind: 'running' } & { healthChecks: string[] })
export type DataUrl = string
export type DataUrlInput = string
export type DeleteLegacyParamsInput = { targetId: BackupTargetIdInput }
export type DependencyMetadata = {
  title: LocaleString | null
  icon: DataUrl | null
  description: LocaleString | null
  optional: boolean
  versionRange: string | null
} & Partial<Exclude<CurrentDependencyKind | null, null>>
export type DerivedAddressInfo = {
  enabled: string[]
  disabled: [string, number][]
  guaWan: string[]
  lanEnabled: [string, number][]
  available: HostnameInfo[]
}
export type Description = { short: LocaleString; long: LocaleString }
export type DeviceFilter = {
  description: string
  class: 'processor' | 'display'
  product: string | null
  vendor: string | null
  capabilities: string[] | null
  driver: string | null
}
export type DeviceInfo = { os: OsInfo; hardware: HardwareInfo | null }
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
export type DownloadsResponse = {
  totalRequests: number
  byPackage: CountEntry[]
  byPackageVersion: PackageVersionCount[]
}
export type Dump = { id: number; value: unknown }
export type DumpParamsInput = { pointer?: string | null }
export type EchoParamsInput = { message: string }
export type EditSignerParamsInput = {
  id: GuidInput
  setName?: string | null
  addContact: ContactInfoInput[]
  addKey: AnyVerifyingKeyInput[]
  removeContact: ContactInfoInput[]
  removeKey: AnyVerifyingKeyInput[]
}
export type EncryptedWireInput = { encrypted: unknown }
export type Epp = string
export type EppInfo = { current: Epp | null; available: Epp[] }
export type EppInput = string
export type EppParamsInput = { set?: EppInput | null }
export type ForgetGatewayParamsInput = { gateway: GatewayIdInput }
export type ForwardTable = { [key: string]: ForwardTarget }
export type ForwardTarget = {
  target: string
  target_prefix: number
  reqs: string
}
export type FullIndex = {
  name: string | null
  icon: DataUrl | null
  description: LocaleString | null
  package: PackageIndex
  os: OsIndex
  signers: { [key: string]: SignerInfo }
}
export type GatewayId = string
export type GatewayIdInput = string
export type GatewayPortMapCapabilities = {
  pcp: CapabilityVerdict
  natPmp: CapabilityVerdict
  upnp: CapabilityVerdict
  pcpHostname: CapabilityVerdict
}
export type GatewayType = 'inbound-outbound' | 'outbound-only'
export type GatewayTypeInput = 'inbound-outbound' | 'outbound-only'
export type GenerateCertParamsInput = { subject: string[] }
export type GenerateCertificateParamsInput = {
  hostnames: string[]
  ed25519?: boolean
}
export type GenerateCertificateResponse = { key: string; fullchain: string }
export type GetActionInputParamsInput = {
  packageId: PackageIdInput
  actionId: ActionIdInput
  prefill?: Record<string, unknown> | null
}
export type GetDownloadsParamsInput = {
  pkgId?: string | null
  version?: string | null
  after?: string | null
  before?: string | null
}
export type GetOsAssetParamsInput = { version: string; platform: string }
export type GetOsVersionParamsInput = {
  sourceVersion?: string | null
  targetVersion?: string | null
  serverId?: string | null
  platform?: string | null
}
export type GetPackageParamsInput = {
  id?: PackageIdInput | null
  targetVersion?: string | null
  sourceVersion?: VersionInput | null
  otherVersions?: PackageDetailLevelInput | null
}
export type GetPackageResponse = {
  categories: string[]
  best: { [key: string]: PackageVersionInfo }
  otherVersions?: { [key: string]: PackageInfoShort } | null
}
export type GetPackageResponseFull = {
  categories: string[]
  best: { [key: string]: PackageVersionInfo }
  otherVersions: { [key: string]: PackageVersionInfo }
}
export type GetPackageResult =
  | GetPackageResponseFull
  | GetPackageResponse
  | { [key: string]: GetPackageResponseFull }
  | { [key: string]: GetPackageResponse }
export type GetUsersParamsInput = {
  after?: string | null
  before?: string | null
}
export type GigaBytes = { value: string; unit: string }
export type GitHash = string
export type Governor = string
export type GovernorInfo = { current: Governor | null; available: Governor[] }
export type GovernorInput = string
export type GovernorParamsInput = { set?: GovernorInput | null }
export type Guid = string
export type GuidInput = string
export type HardwareInfo = {
  arch: string
  ram: number
  devices: LshwDevice[] | null
}
export type HardwareRequirements = {
  device: DeviceFilter[]
  ram: number | null
  arch: string[] | null
}
export type HasUnenrolledKeys = null
export type HostAddress = {
  address: string
  public: PublicDomainConfig | null
  private: GatewayId[] | null
}
export type HostId = string
export type HostIdInput = string
export type HostnameInfo = {
  ssl: boolean
  public: boolean
  hostname: string
  port: number | null
  metadata: HostnameMetadata
}
export type HostnameInfoInput = {
  ssl: boolean
  public: boolean
  hostname: string
  port?: number | null
  metadata: HostnameMetadataInput
}
export type HostnameMetadata =
  | ({ kind: 'ipv4' } & { gateway: GatewayId })
  | ({ kind: 'ipv6' } & { gateway: GatewayId; scopeId: number })
  | ({ kind: 'mdns' } & { gateways: GatewayId[] })
  | ({ kind: 'private-domain' } & { gateways: GatewayId[] })
  | ({ kind: 'public-domain' } & { gateway: GatewayId })
  | ({ kind: 'plugin' } & {
      packageId: PackageId
      removeAction: ActionId | null
      overflowActions: ActionId[]
      info: unknown
    })
export type HostnameMetadataInput =
  | ({ kind: 'ipv4' } & { gateway: GatewayIdInput })
  | ({ kind: 'ipv6' } & { gateway: GatewayIdInput; scopeId: number })
  | ({ kind: 'mdns' } & { gateways: GatewayIdInput[] })
  | ({ kind: 'private-domain' } & { gateways: GatewayIdInput[] })
  | ({ kind: 'public-domain' } & { gateway: GatewayIdInput })
  | ({ kind: 'plugin' } & {
      packageId: PackageIdInput
      removeAction?: ActionIdInput | null
      overflowActions: ActionIdInput[]
      info?: unknown
    })
export type HttpRedirectStatus = {
  ip: string
  enabled: boolean
  forwarded: boolean
}
export type InfoParamsInput = {
  targetId: BackupTargetIdInput
  serverId: string
  password: string
}
export type InitAcmeParamsInput = {
  provider: AcmeProviderInput
  contact: string[]
}
export type InstallParamsInput = {
  registry: string
  id: PackageIdInput
  version: VersionInput
}
export type InstalledVersionParamsInput = { id: PackageIdInput }
export type IoFormatInput =
  | 'json'
  | 'json-pretty'
  | 'yaml'
  | 'cbor'
  | 'toml'
  | 'toml-pretty'
export type IpInfo = {
  name: string
  scopeId: number
  deviceType: NetworkInterfaceType | null
  subnets: string[]
  lanIp: string[]
  wanIp: string | null
  ntpServers: string[]
  dnsServers: string[]
}
export type KeyboardOptionsInput = {
  layout: string
  keymap?: string | null
  model?: string | null
  variant?: string | null
  options?: string[]
}
export type KillParamsInput = { ids: string[] }
export type ListDevicesParamsInput = { subnet: string }
export type ListNotificationParamsInput = {
  before?: number | null
  limit?: number | null
}
export type ListPackageSignersParamsInput = { id: PackageIdInput }
export type ListParamsInput = {}
export type ListVersionSignersParamsInput = { version: string }
export type LocaleString = string | Record<string, string>
export type LocaleStringInput = string | Record<string, string>
export type LogEntry = { timestamp: string; message: string; bootId: string }
export type LogFollowResponse = { startCursor: string | null; guid: Guid }
export type LogResponse = {
  entries: LogEntry[]
  startCursor: string | null
  endCursor: string | null
}
export type LoginParamsInput = {
  password: string
  pubkey: AnyVerifyingKeyInput
  ephemeral?: boolean
}
export type LogoutParamsInput = {}
export type LshwDevice =
  | (
      | ({ class: 'processor' } & Exclude<LshwProcessor, null>)
      | (null extends LshwProcessor ? { class: 'processor' } : never)
    )
  | (
      | ({ class: 'display' } & Exclude<LshwDisplay, null>)
      | (null extends LshwDisplay ? { class: 'display' } : never)
    )
export type LshwDisplay = {
  product: string | null
  vendor: string | null
  capabilities: string[]
  driver: string | null
}
export type LshwProcessor = {
  product: string | null
  vendor: string | null
  capabilities: string[]
}
export type MaybeUtf8String = string | number[]
export type MebiBytes = { value: string; unit: string }
export type MerkleArchiveCommitment = {
  rootSighash: string
  rootMaxsize: number
}
export type MerkleArchiveCommitmentInput = {
  rootSighash: string
  rootMaxsize: number
}
export type Metrics = {
  general: MetricsGeneral
  memory: MetricsMemory
  cpu: MetricsCpu
  disk: MetricsDisk
}
export type MetricsCpu = {
  percentageUsed: Percentage
  idle: Percentage
  userSpace: Percentage
  kernelSpace: Percentage
  wait: Percentage
}
export type MetricsDisk = {
  percentageUsed: Percentage
  used: GigaBytes
  available: GigaBytes
  capacity: GigaBytes
}
export type MetricsFollowParamsInput = {}
export type MetricsFollowResponse = { guid: Guid; metrics: Metrics }
export type MetricsGeneral = { temperature: Celsius | null }
export type MetricsMemory = {
  percentageUsed: Percentage
  total: MebiBytes
  available: MebiBytes
  used: MebiBytes
  zramTotal: MebiBytes
  zramAvailable: MebiBytes
  zramUsed: MebiBytes
}
export type MetricsSummary = {
  totalCheckins: number
  uniqueServers: number
  totalPackageRequests: number
  byArch: CountEntry[]
  byOsVersion: CountEntry[]
}
export type MiB = number
export type ModifyNotificationBeforeParamsInput = { before: number }
export type ModifyNotificationParamsInput = { ids: number[] }
export type MountParamsInput = {
  targetId: BackupTargetIdInput
  serverId?: string | null
  password: string
  allowPartial: boolean
}
export type NetInfo = {
  assignedPort: number | null
  assignedSslPort: number | null
}
export type NetworkInterfaceInfo = {
  name: string | null
  secure: boolean | null
  ipInfo: IpInfo | null
  type: GatewayType
  portMap: GatewayPortMapCapabilities
  dnsUpdate: CapabilityVerdict
}
export type NetworkInterfaceType =
  | 'ethernet'
  | 'wireless'
  | 'bridge'
  | 'wireguard'
  | 'loopback'
export type Notification = {
  packageId: PackageId | null
  createdAt: string
  code: number
  level: NotificationLevel
  title: string
  message: string
  data: unknown
  seen: boolean
}
export type NotificationLevel = 'success' | 'info' | 'warning' | 'error'
export type NotificationLevelInput = 'success' | 'info' | 'warning' | 'error'
export type NotificationWithId = { id: number } & Notification
export type OsIndex = { versions: OsVersionInfoMap }
export type OsInfo = {
  version: Version
  compat: string
  platform: string
  language: string | null
}
export type OsVersionInfo = {
  headline: string
  releaseNotes: string
  sourceVersion: string
  authorized: Guid[]
  iso: {
    [key: string]: {
      publishedAt: string
      urls: string[]
      commitment: Blake3Commitment
      signatures: { [key: string]: AnySignature }
    }
  }
  squashfs: {
    [key: string]: {
      publishedAt: string
      urls: string[]
      commitment: Blake3Commitment
      signatures: { [key: string]: AnySignature }
    }
  }
  img: {
    [key: string]: {
      publishedAt: string
      urls: string[]
      commitment: Blake3Commitment
      signatures: { [key: string]: AnySignature }
    }
  }
}
export type OsVersionInfoMap = { [key: string]: OsVersionInfo }
export type PackageBackupInfo = {
  title: string
  version: Version
  osVersion: string
  timestamp: string
}
export type PackageDetailLevelInput = 'none' | 'short' | 'full'
export type PackageId = string
export type PackageIdInput = string
export type PackageIdParamsInput = { id: PackageIdInput }
export type PackageIndex = {
  categories: { [key: string]: Category }
  packages: { [key: string]: PackageInfo }
}
export type PackageInfo = {
  authorized: { [key: string]: string }
  versions: { [key: string]: PackageVersionInfo }
  categories: string[]
}
export type PackageInfoShort = { releaseNotes: LocaleString }
export type PackageMetadata = {
  title: string
  description: Description
  releaseNotes: LocaleString
  preDownloadAlert?: PreDownloadAlert | null
  gitHash: GitHash | null
  license: string
  packageRepo: string
  upstreamRepo: string
  marketingUrl: string | null
  donationUrl: string | null
  osVersion: string
  sdkVersion: string | null
  hardwareAcceleration: boolean
  userspaceFilesystems: boolean
  virtualNetworking: boolean
  hardwareVirtualization: boolean
  plugins: PluginId[]
  satisfies: Version[]
}
export type PackageVersionCount = {
  pkgId: string
  version: string
  count: number
}
export type PackageVersionInfo = {
  icon: DataUrl
  dependencyMetadata: { [key: string]: DependencyMetadata }
  sourceVersion: string | null
  s9pks: [
    HardwareRequirements,
    {
      publishedAt: string
      urls: string[]
      commitment: MerkleArchiveCommitment
      signatures: { [key: string]: AnySignature }
    },
  ][]
} & PackageMetadata
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
export type PassthroughInfo = {
  hostname: string
  listenPort: number
  backend: string
  publicGateways: string[]
  privateIps: string[]
}
export type PasswordTypeInput = EncryptedWireInput | string
export type Percentage = { value: string; unit: string }
export type PluginId = 'url-v0'
export type PreDownloadAlert = {
  message: LocaleString
  when: PreDownloadAlertWhen
}
export type PreDownloadAlertWhen = { sourceVersion: string }
export type ProxyAuth =
  | ({ type: 'bearer' } & { tokens: string[]; realm: string | null })
  | ({ type: 'basic' } & {
      credentials: BasicCredential[]
      realm: string | null
    })
export type PublicDomainConfig = {
  gateway: GatewayId
  acme: AcmeProvider | null
}
export type QueryDnsParamsInput = { fqdn: string }
export type QueryDnsRes = { ipv4: string | null; ipv6: string | null }
export type RebuildParamsInput = { id: PackageIdInput }
export type RegistryInfo = {
  name: string | null
  icon: DataUrl | null
  description: LocaleString | null
  categories: { [key: string]: Category }
}
export type RegistryUrlParamsInput = { registry: string }
export type RemoveAcmeParamsInput = { provider: AcmeProviderInput }
export type RemoveAdminParamsInput = { signer: GuidInput }
export type RemoveAssetParamsInput = { version: string; platform: string }
export type RemoveCategoryParamsInput = { id: string }
export type RemoveDeviceParamsInput = { subnet: string; ip: string }
export type RemoveDnsRecordParamsInput = { name: string; type?: string | null }
export type RemoveDomainParamsInput = { fqdn: string }
export type RemoveKeyParamsInput = { key: AnyVerifyingKeyInput }
export type RemoveMirrorParamsInput = {
  id: PackageIdInput
  version: VersionInput
  url: string
}
export type RemovePackageFromCategoryParamsInput = {
  id: string
  package: PackageIdInput
}
export type RemovePackageParamsInput = {
  id: PackageIdInput
  version?: VersionInput | null
  sighash?: string | null
  force?: boolean
}
export type RemovePackageSignerParamsInput = {
  id: PackageIdInput
  signer: GuidInput
}
export type RemovePassthroughParamsInput = {
  hostname: string
  'listen-port': number
}
export type RemovePinholeParamsInput = { gua: string; externalPort: number }
export type RemovePortForwardParamsInput = {
  source: string
  hostname?: string | null
}
export type RemoveSignerParamsInput = { id: GuidInput }
export type RemoveTunnelParamsInput = { id: GatewayIdInput }
export type RemoveVersionParamsInput = { version: string }
export type RenameGatewayParamsInput = { id: GatewayIdInput; name: string }
export type ReplayIdInput = string
export type RequiresHostIdInput = { host: HostIdInput }
export type RequiresPackageIdInput = { package: PackageIdInput }
export type RequiresReboot = boolean
export type ResetPasswordParamsInput = {
  newPassword?: PasswordTypeInput | null
}
export type RestorePackageParamsInput = {
  targetId: BackupTargetIdInput
  password: string
  ids: PackageIdInput[]
  serverId?: string | null
}
export type RunActionParamsInput = {
  packageId: PackageIdInput
  eventId?: GuidInput | null
  actionId: ActionIdInput
  input?: unknown | null
}
export type Security = { ssl: boolean }
export type ServerHostname = string
export type ServiceInterface = {
  id: ServiceInterfaceId
  name: string
  description: string
  masked: boolean
  addressInfo: AddressInfo
  type: ServiceInterfaceType
  preferredLauncherAddress: string | null
}
export type ServiceInterfaceId = string
export type ServiceInterfaceType = 'ui' | 'p2p' | 'api'
export type ServiceStats = {
  container_id: ContainerId
  memory_usage: MiB
  memory_limit: MiB
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
export type SetCountryParamsInput = { country: string }
export type SetDefaultOutboundParamsInput = { gateway?: GatewayIdInput | null }
export type SetDescriptionParamsInput = { description: LocaleStringInput }
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
export type SetEchoipUrlsParamsInput = { urls: string[] }
export type SetGatewaySecureParamsInput = {
  gateway: GatewayIdInput
  secure?: boolean | null
}
export type SetHttpRedirectEnabledParamsInput = { ip: string; enabled: boolean }
export type SetIconParamsInput = { icon: DataUrlInput }
export type SetLanguageParamsInput = { language: string }
export type SetListenParamsInput = { listen: string }
export type SetNameParamsInput = { name: string }
export type SetOutboundGatewayParamsInput = {
  package: PackageIdInput
  gateway?: GatewayIdInput | null
}
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
export type SetServerHostnameParamsInput = { hostname: string }
export type SetStaticDnsParamsInput = { servers?: string[] | null }
export type SetSubnetDnsParamsInput = {
  mode: DnsModeInput
  deviceIp?: string | null
  servers: string[]
}
export type SetSubnetIpv6ParamsInput = { prefix?: string | null }
export type SetSubnetWanParamsInput = { wanIp?: string | null }
export type SetWifiEnabledParamsInput = { enabled: boolean }
export type ShowConfigParamsInput = { subnet: string; ip: string }
export type ShutdownParamsInput = { wait?: boolean }
export type SideloadParamsInput = {}
export type SideloadResponse = { upload: Guid; progress: Guid }
export type SignAssetParamsInput = {
  version: string
  platform: string
  signature: AnySignatureInput
}
export type SignalStrength = number
export type SignerInfo = {
  name: string
  contact: ContactInfo[]
  keys: AnyVerifyingKey[]
}
export type SignerInfoInput = {
  name: string
  contact: ContactInfoInput[]
  keys: AnyVerifyingKeyInput[]
}
export type SmtpSecurityInput = 'starttls' | 'tls'
export type SmtpValueInput = {
  host?: string
  server?: string
  port: number
  from: string
  login?: string
  username?: string
  password?: string | null
  security?: SmtpSecurityInput
} & ({ host: string } | { server: string }) &
  ({ login: string } | { username: string })
export type SshAddParamsInput = { key: SshPubKeyInput }
export type SshDeleteParamsInput = { fingerprint: string }
export type SshKeyResponse = {
  alg: string
  fingerprint: string
  hostname: string
  createdAt: string
}
export type SshPubKeyInput = string
export type Ssid = string
export type StartOsRecoveryInfo = {
  hostname: ServerHostname
  version: string
  timestamp: string
}
export type StartParamsInput = { id: PackageIdInput; force?: boolean }
export type SubnetParamsInput = { subnet: string }
export type SubscribeParamsInput = { pointer?: string | null }
export type SubscribeRes = { dump: Dump; guid: Guid }
export type TermSizeInput = {
  rows: number
  cols: number
  pixels?: [number, number] | null
}
export type TestSmtpParamsInput = {
  host: string
  port: number
  from: string
  to: string
  username: string
  password: string
  security?: SmtpSecurityInput
}
export type TimeInfo = { now: string; uptime: number }
export type TrustCaParamsInput = { cert: string }
export type TrustedCa = { subject: string; fingerprint: string }
export type TunnelCertDataInput = { key: string; cert: string }
export type TunnelUpdateResult = {
  status: string
  installed: string
  candidate: string
}
export type TunnelUrlParamsInput = { tunnel: string }
export type UiParamsInput = { pointer: string; value: unknown }
export type UmountParamsInput = { targetId?: BackupTargetIdInput | null }
export type UninstallParamsInput = {
  id: PackageIdInput
  soft?: boolean
  force?: boolean
}
export type UnsetGatewaySecureParamsInput = { gateway: GatewayIdInput }
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
export type UpdateSystemParamsInput = {
  registry: string
  targetVersion?: string | null
  progress?: boolean
}
export type UpdateSystemRes = { target: string | null; progress: string | null }
export type UpdateTunnelParamsInput = { id: GatewayIdInput; config: string }
export type UpstreamCertValidation = 'disable' | { certificate: string }
export type UsersResponse = { uniqueServers: number; totalCheckins: number }
export type Version = string
export type VersionInput = string
export type VersionSignerParamsInput = { version: string; signer: GuidInput }
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
export type WifiAddParamsInput = { ssid: string; password: string }
export type WifiListInfo = {
  ssids: { [key: string]: SignalStrength }
  connected: Ssid | null
  country: string | null
  ethernet: boolean
  availableWifi: WifiListOut[]
}
export type WifiListOut = {
  ssid: Ssid
  strength: SignalStrength
  security: string[]
}
export type WifiSsidParamsInput = { ssid: string }
export type ZramParamsInput = { enable: boolean }
