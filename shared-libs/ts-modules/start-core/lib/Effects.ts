import {
  ActionId,
  ActionInput,
  ActionMetadata,
  SetMainStatus,
  DependencyRequirement,
  CheckDependenciesResult,
  CreateNotificationParams,
  SetHealth,
  BindParams,
  BindRangeParams,
  RetireHostParams,
  RetireBindingParams,
  HostId,
  NetInfo,
  Host,
  ExportServiceInterfaceParams,
  ExportRangeServiceInterfaceParams,
  ServiceInterface,
  CreateTaskParams,
  MountParams,
  StatusInfo,
  Manifest,
  HostnameInfo,
  Progress,
} from './osBindings'
import {
  PackageId,
  Dependencies,
  ServiceInterfaceId,
  SmtpValue,
  ActionResult,
  PluginHostnameInfo,
} from './types'

/** Used to reach out from the pure js runtime */

export type Effects = {
  readonly eventId: string | null
  child: (name: string) => Effects
  constRetry?: () => void
  isInContext: boolean
  onLeaveContext: (fn: () => void | null | undefined) => void
  clearCallbacks: (
    options: { only: number[] } | { except: number[] },
  ) => Promise<null>

  // action
  action: {
    /** Define an action that can be invoked by a user or service */
    export(options: { id: ActionId; metadata: ActionMetadata }): Promise<null>
    /** Remove all exported actions */
    clear(options: { except: ActionId[] }): Promise<null>
    getInput(options: {
      packageId?: PackageId
      actionId: ActionId
    }): Promise<ActionInput | null>
    run<Input extends Record<string, unknown>>(options: {
      packageId?: PackageId
      actionId: ActionId
      input?: Input
    }): Promise<ActionResult | null>
    createTask(options: CreateTaskParams): Promise<null>
    clearTasks(
      options: { only: string[] } | { except: string[] },
    ): Promise<null>
  }

  // control
  /** restart this service's main function */
  restart(): Promise<null>
  /** stop this service's main function */
  shutdown(): Promise<null>
  /** ask the host os what the service's current status is */
  getStatus(options: {
    packageId?: PackageId
    callback?: () => void
  }): Promise<StatusInfo | null>
  /** DEPRECATED: indicate to the host os what runstate the service is in */
  setMainStatus(options: SetMainStatus): Promise<null>

  // dependency
  /** Set the dependencies of what the service needs, usually run during the inputSpec action as a best practice */
  setDependencies(options: { dependencies: Dependencies }): Promise<null>
  /** Get the list of the dependencies, both the dynamic set by the effect of setDependencies and the end result any required in the manifest  */
  getDependencies(): Promise<DependencyRequirement[]>
  /** Test whether current dependency requirements are satisfied */
  checkDependencies(options: {
    packageIds?: PackageId[]
  }): Promise<CheckDependenciesResult[]>
  /** mount a volume of a dependency */
  mount(options: MountParams): Promise<string>
  /** Returns a list of the ids of all installed packages */
  getInstalledPackages(): Promise<string[]>
  /** Returns the manifest of a service */
  getServiceManifest(options: {
    packageId: PackageId
    callback?: () => void
  }): Promise<Manifest>

  // backup
  /**
   * Low-level backup-progress report. **Prefer `FullProgressTracker`** — the
   * backup harness hands each hook a tracker whose phase updates auto-report
   * via this effect. Service code should not call this directly.
   *
   * The host stores `progress` as the service's phase in the server-wide
   * backup tracker (same `FullProgress` wire format used by installs/updates).
   * `progress` is a leaf `Progress` or a nested `FullProgress`. No-op outside
   * the backup transition.
   */
  setBackupProgress(o: { progress: Progress }): Promise<null>

  // init
  /**
   * Low-level init-progress report. **Prefer `FullProgressTracker`** — the init
   * harness hands each init handler (and migration) a tracker whose phase
   * updates auto-report via this effect. Service code should not call this
   * directly.
   *
   * The host stores `progress` as this service's install/update finalization
   * phase, so the UI surfaces it in the "Installing" / "Updating" phase of the
   * install progress. `progress` is a leaf `Progress` or a nested
   * `FullProgress`. No-op outside the init transition.
   */
  setInitProgress(o: { progress: Progress }): Promise<null>

  // health
  /** sets the result of a health check */
  setHealth(o: SetHealth): Promise<null>

  // notification
  notification: {
    /**
     * Create a notification attributed to this service.
     *
     * Omit `data` for a plain notification (panel row shows `title` and
     * `message` only). Pass `data` as markdown text to attach a long-form
     * body that the UI renders in a "View Details" modal (release notes,
     * post-update changelogs, structured error reports).
     */
    create(options: CreateNotificationParams): Promise<null>
  }

  // subcontainer
  subcontainer: {
    /** A low level api used by SubContainer */
    createFs(options: {
      imageId: string
      name: string | null
    }): Promise<[string, string]>
    /** A low level api used by SubContainer */
    destroyFs(options: { guid: string }): Promise<null>
  }

  // net
  // bind
  /** Creates a host connected to the specified port with the provided options */
  bind(options: BindParams): Promise<null>
  /**
   * Binds a contiguous range of UDP+TCP ports to the specified host. Used
   * for real-time / WebRTC servers (coturn, RTP, SIP) and other pooled-port
   * protocols (bitcoin ZMQ, FTP data) that need a public port range.
   * `externalStartPort` may differ from `internalStartPort` — the forward
   * maps the external range onto the internal range by offset. The whole
   * range is allocated atomically; any partial collision with already-bound
   * external ports is a hard error. Capped at 500 ports per call.
   */
  bindRange(options: BindRangeParams): Promise<null>
  /**
   * The external ports assigned to a binding, or `null` if the host or the
   * binding does not exist. Raw allocator metadata — to reach a dependency,
   * use `sdk.host.getBridgeAddress`, which is correct regardless of how the
   * dependency terminates TLS.
   */
  getServicePortForward(options: {
    packageId?: PackageId
    hostId: HostId
    internalPort: number
  }): Promise<NetInfo | null>
  /**
   * Disables every network binding except those listed, keeping each row, its
   * external port claim and the operator's per-address choices. Called at the
   * end of a `setupInterfaces` pass. To delete one for good, see `retireHost` /
   * `retireBinding`.
   */
  clearBindings(options: {
    except: { id: HostId; internalPort: number }[]
  }): Promise<null>
  /**
   * Permanently removes a host and everything under it: every binding and port
   * range, their exported service interfaces, the operator's public and private
   * domains for the host, and their per-address enable/disable choices. The
   * external ports return to the server's pool.
   *
   * Where `clearBindings` disables — keeping the row, its port claim and the
   * operator's choices so the address survives a pass that did not declare it —
   * this deletes. Binding the same id afterwards starts a fresh host that may
   * be assigned a different external port.
   *
   * Resolves `false` if there was no such host, so a migration re-run is safe.
   */
  retireHost(options: RetireHostParams): Promise<boolean>
  /**
   * Permanently removes whichever of the single-port binding and the port range
   * is bound at `internalPort` — and both, if both are — along with their
   * exported service interfaces. The external ports return to the server's
   * pool. The host survives, keeping its domains.
   *
   * Resolves `false` if nothing was bound at `internalPort`.
   */
  retireBinding(options: RetireBindingParams): Promise<boolean>
  // host
  /** Returns information about the specified host, if it exists */
  getHostInfo(options: {
    packageId?: PackageId
    hostId: HostId
    callback?: () => void
  }): Promise<Host | null>
  /** Returns the IP address of the container */
  getContainerIp(options: {
    packageId?: PackageId
    callback?: () => void
  }): Promise<string>
  /** Returns the IP address of StartOS */
  getOsIp(): Promise<string>
  /** Returns the effective outbound gateway for this service */
  getOutboundGateway(options: { callback?: () => void }): Promise<string>
  // interface
  /** Creates an interface bound to a specific host and port to show to the user */
  exportServiceInterface(options: ExportServiceInterfaceParams): Promise<null>
  /**
   * Exports the single restricted `api` interface for a port-range binding,
   * stored on the range it belongs to (`RangeOrigin.export`).
   */
  exportRangeServiceInterface(
    options: ExportRangeServiceInterfaceParams,
  ): Promise<null>
  /** Returns an exported service interface */
  getServiceInterface(options: {
    packageId?: PackageId
    serviceInterfaceId: ServiceInterfaceId
    callback?: () => void
  }): Promise<ServiceInterface | null>
  /** Returns all exported service interfaces for a package */
  listServiceInterfaces(options: {
    packageId?: PackageId
    callback?: () => void
  }): Promise<Record<ServiceInterfaceId, ServiceInterface>>
  /** Removes all service interfaces */
  clearServiceInterfaces(options: {
    except: ServiceInterfaceId[]
  }): Promise<null>

  plugin: {
    url: {
      register(options: { tableAction: ActionId }): Promise<null>
      exportUrl(options: {
        hostnameInfo: PluginHostnameInfo
        removeAction: ActionId | null
        overflowActions: ActionId[]
      }): Promise<null>
      clearUrls(options: { except: PluginHostnameInfo[] }): Promise<null>
    }
  }
  // ssl
  /** Returns a PEM encoded fullchain for the hostnames specified */
  getSslCertificate: (options: {
    hostnames: string[]
    algorithm?: 'ecdsa' | 'ed25519'
    callback?: () => void
  }) => Promise<[string, string, string]>
  /** Returns a PEM encoded private key corresponding to the certificate for the hostnames specified */
  getSslKey: (options: {
    hostnames: string[]
    algorithm?: 'ecdsa' | 'ed25519'
  }) => Promise<string>

  /** sets the version that this service's data has been migrated to */
  setDataVersion(options: { version: string | null }): Promise<null>
  /** returns the version that this service's data has been migrated to */
  getDataVersion(): Promise<string | null>

  // system
  /** Returns globally configured SMTP settings, if they exist */
  getSystemSmtp(options: { callback?: () => void }): Promise<SmtpValue | null>
}
