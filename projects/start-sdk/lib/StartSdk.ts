import * as fs from 'node:fs/promises'
import * as actions from '@start9labs/start-core/actions'
import { InputSpec } from '@start9labs/start-core/actions/input/builder/inputSpec'
import { List } from '@start9labs/start-core/actions/input/builder/list'
import { Value } from '@start9labs/start-core/actions/input/builder/value'
import { Variants } from '@start9labs/start-core/actions/input/builder/variants'
import {
  customSmtp,
  smtpInputSpec,
  smtpProviderVariants,
  systemSmtpSpec,
} from '@start9labs/start-core/actions/input/inputSpecConstants'
import {
  Action,
  ActionInfo,
  Actions,
  MaybeFn,
  Run,
} from '@start9labs/start-core/actions/setupActions'
import {
  CheckDependencies,
  checkDependencies,
} from '@start9labs/start-core/dependencies/dependencies'
import {
  CurrentDependenciesResult,
  ValidateVersionRanges,
  setupDependencies,
} from '@start9labs/start-core/dependencies/setupDependencies'
import { testTypeVersion } from '@start9labs/start-core/exver'
import {
  setupInit,
  setupOnInit,
  setupOnUninit,
  setupUninit,
} from '@start9labs/start-core/inits'
import { MultiHost, Scheme } from '@start9labs/start-core/interfaces/Host'
import { RangeInterfaceBuilder } from '@start9labs/start-core/interfaces/RangeInterfaceBuilder'
import { ServiceInterfaceBuilder } from '@start9labs/start-core/interfaces/ServiceInterfaceBuilder'
import { setupExportedUrls } from '@start9labs/start-core/interfaces/setupExportedUrls'
import { setupServiceInterfaces } from '@start9labs/start-core/interfaces/setupInterfaces'
import * as T from '@start9labs/start-core/types'
import { Effects, ServiceInterfaceType } from '@start9labs/start-core/types'
import { GetContainerIp } from '@start9labs/start-core/util/GetContainerIp'
import { getHost, getOwnHost } from '@start9labs/start-core/util/GetHostInfo'
import { GetStatus } from '@start9labs/start-core/util/GetStatus'
import * as patterns from '@start9labs/start-core/util/patterns'
import { Backups } from './backup/Backups'
import { SetupBackupsParams, setupBackups } from './backup/setupBackups'
import { checkWebUrl, runHealthScript } from './health/checkFns'
import { checkPortListening } from './health/checkFns/checkPortListening'
import { setupMain } from './mainFn'
import { Daemon, Daemons } from './mainFn/Daemons'
import { Mounts } from './mainFn/Mounts'
import { cooldownTrigger, statusTrigger } from './trigger'
import { defaultTrigger } from './trigger/defaultTrigger'
import {
  GetOutboundGateway,
  GetSslCertificate,
  GetSystemSmtp,
  getServiceManifest,
  nullIfEmpty,
  splitCommand,
} from './util'
import {
  CommandOptions,
  ExitError,
  SubContainer,
  SubContainerEager,
  SubContainerLazy,
} from './util/SubContainer'
import { createVolumes } from './util/Volume'
import { getDataVersion, setDataVersion } from './version'

/** The minimum StartOS version required by this SDK release */
export const OSVersion = testTypeVersion('0.4.0-beta.10')

// prettier-ignore
type AnyNeverCond<T extends any[], Then, Else> = 
    T extends [] ? Else :
    T extends [never, ...Array<any>] ? Then :
    T extends [any, ...infer U] ? AnyNeverCond<U,Then, Else> :
    never

/**
 * The top-level SDK facade for building StartOS service packages.
 *
 * Use `StartSdk.of()` to create an uninitialized instance, then call `.withManifest()`
 * to bind it to a manifest, and finally `.build()` to obtain the full toolkit of helpers
 * for actions, daemons, backups, interfaces, health checks, and more.
 *
 * @typeParam Manifest - The service manifest type; starts as `never` until `.withManifest()` is called.
 */
export class StartSdk<Manifest extends T.SDKManifest> {
  private constructor(readonly manifest: Manifest) {}
  /**
   * Create an uninitialized StartSdk instance. Call `.withManifest()` next.
   * @returns A new StartSdk with no manifest bound.
   */
  static of() {
    return new StartSdk<never>(null as never)
  }
  /**
   * Bind a manifest to the SDK, producing a typed SDK instance.
   * @param manifest - The service manifest definition
   * @returns A new StartSdk instance parameterized by the given manifest type
   */
  withManifest<Manifest extends T.SDKManifest = never>(manifest: Manifest) {
    return new StartSdk<Manifest>(manifest)
  }

  private ifPluginEnabled<P extends T.PluginId, T>(
    plugin: P,
    value: T,
  ): Manifest extends { plugins: P[] } ? T : null {
    if (this.manifest.plugins?.includes(plugin)) return value as any
    return null as any
  }

  /**
   * Finalize the SDK and return the full set of helpers for building a StartOS service.
   *
   * This method is only callable after `.withManifest()` has been called (enforced at the type level).
   *
   * @param isReady - Type-level gate; resolves to `true` only when a manifest is bound.
   * @returns An object containing all SDK utilities: actions, daemons, backups, interfaces, health checks, volumes, triggers, and more.
   */
  build(isReady: AnyNeverCond<[Manifest], 'Build not ready', true>) {
    type NestedEffects =
      | 'subcontainer'
      | 'store'
      | 'action'
      | 'plugin'
      | 'notification'
    type InterfaceEffects =
      | 'getServiceInterface'
      | 'listServiceInterfaces'
      | 'exportServiceInterface'
      | 'exportRangeServiceInterface'
      | 'clearServiceInterfaces'
      | 'bind'
      | 'bindRange'
      | 'getHostInfo'
    type MainUsedEffects = 'setMainStatus'
    type CallbackEffects =
      | 'child'
      | 'constRetry'
      | 'isInContext'
      | 'onLeaveContext'
      | 'clearCallbacks'
    type AlreadyExposed =
      | 'getSslCertificate'
      | 'getSystemSmtp'
      | 'getOutboundGateway'
      | 'getContainerIp'
      | 'getStatus'
      | 'getDataVersion'
      | 'setDataVersion'
      | 'getServiceManifest'

    // prettier-ignore
    type StartSdkEffectWrapper = {
      [K in keyof Omit<Effects, "eventId" | NestedEffects | InterfaceEffects | MainUsedEffects | CallbackEffects | AlreadyExposed>]: (effects: Effects, ...args: Parameters<Effects[K]>) => ReturnType<Effects[K]>
    }
    const startSdkEffectWrapper: StartSdkEffectWrapper = {
      restart: (effects, ...args) => effects.restart(...args),
      setDependencies: (effects, ...args) => effects.setDependencies(...args),
      checkDependencies: (effects, ...args) =>
        effects.checkDependencies(...args),
      mount: (effects, ...args) => effects.mount(...args),
      getInstalledPackages: (effects, ...args) =>
        effects.getInstalledPackages(...args),
      getServicePortForward: (effects, ...args) =>
        effects.getServicePortForward(...args),
      clearBindings: (effects, ...args) => effects.clearBindings(...args),
      getOsIp: (effects, ...args) => effects.getOsIp(...args),
      getSslKey: (effects, ...args) => effects.getSslKey(...args),
      shutdown: (effects, ...args) => effects.shutdown(...args),
      getDependencies: (effects, ...args) => effects.getDependencies(...args),
      setHealth: (effects, ...args) => effects.setHealth(...args),
      setBackupProgress: (effects, ...args) =>
        effects.setBackupProgress(...args),
      setInitProgress: (effects, ...args) => effects.setInitProgress(...args),
    }

    return {
      /** The bound service manifest */
      manifest: this.manifest,
      /** Volume path helpers derived from the manifest volume definitions */
      volumes: createVolumes(this.manifest),
      ...startSdkEffectWrapper,
      /** Persist the current data version to the StartOS effect system */
      setDataVersion,
      /** Retrieve the current data version from the StartOS effect system */
      getDataVersion,
      action: {
        /** Execute an action by its ID, optionally providing input */
        run: actions.runAction,
        /** Create a task notification for a specific package's action */
        createTask: <T extends ActionInfo<T.ActionId, any>>(
          effects: T.Effects,
          packageId: T.PackageId,
          action: T,
          severity: T.TaskSeverity,
          options?: actions.TaskOptions<T>,
        ) =>
          actions.createTask({
            effects,
            packageId,
            action,
            severity,
            options: options,
          }),
        /** Create a task notification for this service's own action (uses manifest.id automatically) */
        createOwnTask: <T extends ActionInfo<T.ActionId, any>>(
          effects: T.Effects,
          action: T,
          severity: T.TaskSeverity,
          options?: actions.TaskOptions<T>,
        ) =>
          actions.createTask({
            effects,
            packageId: this.manifest.id,
            action,
            severity,
            options: options,
          }),
        /**
         * Clear one or more task notifications by their replay IDs
         * @param effects - The effects context
         * @param replayIds - One or more replay IDs of the tasks to clear
         */
        clearTask: (effects: T.Effects, ...replayIds: string[]) =>
          effects.action.clearTasks({ only: replayIds }),
      },
      notification: {
        /**
         * @description Create a notification attributed to this service. The
         *   notification appears in the StartOS notifications panel just like
         *   the ones StartOS itself generates (e.g. on backup completion).
         *
         *   Omit `data` for a plain notification — the panel row shows
         *   `title` and `message` only.
         *
         *   Pass `data` as markdown text to attach a long-form body. The
         *   panel row still shows `title` and `message`; clicking
         *   "View Details" opens `data` rendered as markdown in a large
         *   modal. Use this for release notes, post-update changelogs,
         *   structured error reports — long-form content, not a short
         *   status string.
         * @example
         * ```
         *   await sdk.notification.create(effects, {
         *     level: 'info',
         *     title: 'Sync Complete',
         *     message: 'Initial block download finished.',
         *   })
         * ```
         * @example
         * ```
         *   await sdk.notification.create(effects, {
         *     level: 'success',
         *     title: 'Update Complete',
         *     message: 'Hello World was updated to 2.0.0. Tap for release notes.',
         *     data: [
         *       '## What\'s new in 2.0.0',
         *       '',
         *       '- Faster sync on slow networks',
         *       '- New `--verbose` flag for the daemon',
         *       '- Fixed a crash on startup when the data volume was empty',
         *       '',
         *       'See the full changelog at https://example.com/changelog.',
         *     ].join('\n'),
         *   })
         * ```
         */
        create: (effects: T.Effects, options: T.CreateNotificationParams) =>
          effects.notification.create(options),
      },
      /**
       * Check whether the specified (or all) dependencies are satisfied.
       * @param effects - The effects context
       * @param packageIds - Optional subset of dependency IDs to check; defaults to all
       * @returns An object describing which dependencies are satisfied and which are not
       */
      checkDependencies: checkDependencies as <
        DependencyId extends keyof Manifest['dependencies'] & T.PackageId =
          keyof Manifest['dependencies'] & T.PackageId,
      >(
        effects: Effects,
        packageIds?: DependencyId[],
      ) => Promise<CheckDependencies<DependencyId>>,
      host: {
        /**
         * Retrieve one of this package's own hosts by id, with reactive read
         * strategies (`const`/`once`/`watch`/`onChange`/`waitFor`). Reach an
         * exported service interface by walking the host:
         * `host.bindings[internalPort].interfaces[id]` (single-port) or
         * `host.bindingRanges[internalStartPort].interface` (port range).
         *
         * Pass an optional `map` (and `eq`) to react to only a slice of the
         * host — with `const()`, the calling context re-runs when the mapped
         * value changes rather than on any change to the whole host.
         *
         * @param effects - The effects context
         * @param hostId - The host id passed to `sdk.MultiHost.of`
         * @param map - optional selector narrowing reactivity to a child attr
         * @param eq - optional equality for the mapped value (default deep-equal)
         */
        getOwn: getOwnHost,
        /**
         * Retrieve a host from any package by id (defaults to this package when
         * `packageId` is omitted), with the same reactive read strategies and
         * the same optional `map`/`eq` selective reactivity as `getOwn`.
         *
         * @param effects - The effects context
         * @param opts - `{ hostId, packageId? }`
         * @param map - optional selector narrowing reactivity to a child attr
         * @param eq - optional equality for the mapped value (default deep-equal)
         */
        get: getHost,
      },
      /**
       * Get the container IP address with reactive subscription support.
       *
       * Returns an object with multiple read strategies: `const()` for a value
       * that retries on change, `once()` for a single read, `watch()` for an async
       * generator, `onChange()` for a callback, and `waitFor()` to block until a predicate is met.
       *
       * @param effects - The effects context
       * @param options - Optional filtering options (e.g. `containerId`)
       */
      getContainerIp: (
        effects: T.Effects,
        options: Omit<
          Parameters<T.Effects['getContainerIp']>[0],
          'callback'
        > = {},
      ) => new GetContainerIp(effects, options),

      /**
       * Get the service's current status with reactive subscription support.
       *
       * Returns an object with multiple read strategies: `const()` for a value
       * that retries on change, `once()` for a single read, `watch()` for an async
       * generator, `onChange()` for a callback, and `waitFor()` to block until a predicate is met.
       *
       * @param effects - The effects context
       * @param options - Optional filtering options (e.g. `packageId`)
       */
      getStatus: (
        effects: T.Effects,
        options: Omit<Parameters<T.Effects['getStatus']>[0], 'callback'> = {},
      ) => new GetStatus(effects, options),

      MultiHost: {
        /**
         * Create a new MultiHost instance for binding ports and exporting interfaces.
         * @param effects - The effects context
         * @param id - A unique identifier for this multi-host group
         */
        of: (effects: Effects, id: string) => new MultiHost({ id, effects }),
      },
      /**
       * Return `null` if the given string is empty, otherwise return the string unchanged.
       * Useful for converting empty user input into explicit null values.
       */
      nullIfEmpty,
      /**
       * Indicate that a daemon should use the container image's configured entrypoint.
       * @param overrideCmd - Optional command arguments to append after the entrypoint
       */
      useEntrypoint: (overrideCmd?: string[]) =>
        new T.UseEntrypoint(overrideCmd),
      /**
       * @description Use this class to create an Action. By convention, each Action should receive its own file.
       *
       */
      Action: {
        /**
         * @description Use this function to create an action that accepts form input
         * @param id - a unique ID for this action
         * @param metadata - information describing the action and its availability
         * @param inputSpec - define the form input using the InputSpec and Value classes
         * @param prefillFn - optionally fetch data from the file system to pre-fill the input form. Must returns a deep partial of the input spec
         * @param executionFn - execute the action. Optionally return data for the user to view. Must be in the structure of an ActionResult, version "1"
         * @example
         * In this example, we create an action for a user to provide their name.
         *   We prefill the input form with their existing name from the service's yaml file.
         *   The new name is saved to the yaml file, and we return nothing to the user, which
         *   means they will receive a generic success message.
         * 
         * ```
          import { sdk } from '../sdk'
          import { yamlFile } from '../file-models/config.yml'

          const { InputSpec, Value } = sdk

          export const inputSpec = InputSpec.of({
            name: Value.text({
              name: 'Name',
              description:
                'When you launch the Hello World UI, it will display "Hello [Name]"',
              required: true,
              default: 'World',
            }),
          })

          export const setName = sdk.Action.withInput(
            // id
            'set-name',

            // metadata
            async ({ effects }) => ({
              name: 'Set Name',
              description: 'Set your name so Hello World can say hello to you',
              warning: null,
              allowedStatuses: 'any',
              group: null,
              visibility: 'enabled',
            }),

            // form input specification
            inputSpec,

            // optionally pre-fill the input form
            async ({ effects }) => {
              const name = await yamlFile.read.const(effects)?.name
              return { name }
            },

            // the execution function
            async ({ effects, input }) => yamlFile.merge(input)
          )
         * ```
        */
        withInput: Action.withInput,
        /**
         * @description Use this function to create an action that does not accept form input
         * @param id - a unique ID for this action
         * @param metadata - information describing the action and its availability
         * @param executionFn - execute the action. Optionally return data for the user to view. Must be in the structure of an ActionResult, version "1"
         * @example
         * In this example, we create an action that returns a secret phrase for the user to see.
         * 
         * ```
          import { store } from '../file-models/store.json'
          import { sdk } from '../sdk'

          export const showSecretPhrase = sdk.Action.withoutInput(
            // id
            'show-secret-phrase',

            // metadata
            async ({ effects }) => ({
              name: 'Show Secret Phrase',
              description: 'Reveal the secret phrase for Hello World',
              warning: null,
              allowedStatuses: 'any',
              group: null,
              visibility: 'enabled',
            }),

            // the execution function
            async ({ effects }) => ({
              version: '1',
              title: 'Secret Phrase',
              message:
                'Below is your secret phrase. Use it to gain access to extraordinary places',
              result: {
                type: 'single',
                value: (await store.read.once())?.secretPhrase,
                copyable: true,
                qr: true,
                masked: true,
              },
            }),
          )
         * ```
        */
        withoutInput: <Id extends T.ActionId>(
          id: Id,
          metadata: MaybeFn<Omit<T.ActionMetadata, 'hasInput'>>,
          run: Run<{}>,
        ) => Action.withoutInput(id, metadata, run),
      },
      inputSpecConstants: {
        smtpInputSpec,
        systemSmtpSpec,
        customSmtp,
        smtpProviderVariants,
      },
      /**
       * @description Use this function to create a service interface.
       * @param effects
       * @param options
       * @example
       * In this example, we create a standard web UI
       *
       * ```
        const ui = sdk.createInterface(effects, {
          name: 'Web UI',
          id: 'ui',
          description: 'The primary web app for this service.',
          type: 'ui',
          masked: false,
          schemeOverride: null,
          username: null,
          path: '',
          query: {},
        })
       * ```
       */
      createInterface: (
        effects: Effects,
        options: {
          /** The human readable name of this service interface. */
          name: string
          /** A unique ID for this service interface. */
          id: string
          /** The human readable description. */
          description: string
          /** Affects how the interface appears to the user. One of: 'ui', 'api', 'p2p'. If 'ui', the user will see an option to open the UI in a new tab */
          type: ServiceInterfaceType
          /** (optional) prepends the provided username to all URLs. */
          username: null | string
          /** (optional) appends the provided path to all URLs. */
          path: string
          /** (optional) appends the provided query params to all URLs. */
          query: Record<string, string>
          /** (optional) overrides the protocol prefix provided by the bind function.
           *
           * @example `{ ssl: 'ftps', noSsl: 'ftp' }`
           */
          schemeOverride: { ssl: Scheme; noSsl: Scheme } | null
          /** mask the url (recommended if it contains credentials such as an API key or password) */
          masked: boolean
        },
      ) => new ServiceInterfaceBuilder({ ...options, effects }),
      /**
       * A function for creating the single, restricted service interface a port
       * range exports. Pass the result to `RangeOrigin.export` (the handle
       * returned by `MultiHost.bindPortRange`). A range interface is always
       * `api`-typed and has no masked/username/path/query/SSL — its address is
       * the host plus the range's external port span.
       *
       * @example
       * ```
        const range = await sdk.MultiHost.of(effects, 'zmq').bindPortRange({
          internalStartPort: 28332,
          externalStartPort: 28332,
          numberOfPorts: 2,
        })
        await range.export(
          sdk.createRangeInterface(effects, {
            id: 'zmq',
            name: 'ZMQ',
            description: 'Bitcoin ZMQ notification endpoints',
            scheme: 'tcp',
          }),
        )
       * ```
       */
      createRangeInterface: (
        effects: Effects,
        options: {
          /** A unique ID for this range service interface. */
          id: string
          /** The human readable name of this range service interface. */
          name: string
          /** The human readable description. */
          description: string
          /** (optional) transport scheme prefix, e.g. `'tcp'` for bitcoin ZMQ
           * endpoints. Omit for raw UDP/TCP ranges (coturn, RTP, FTP). */
          scheme?: string | null
        },
      ) => new RangeInterfaceBuilder({ ...options, effects }),
      /**
       * Get the system SMTP configuration with reactive subscription support.
       * @param effects - The effects context
       */
      getSystemSmtp: <E extends Effects>(effects: E) =>
        new GetSystemSmtp(effects),
      /**
       * Get the outbound network gateway address with reactive subscription support.
       * @param effects - The effects context
       */
      getOutboundGateway: <E extends Effects>(effects: E) =>
        new GetOutboundGateway(effects),
      /**
       * Get an SSL certificate for the given hostnames with reactive subscription support.
       * @param effects - The effects context
       * @param hostnames - The hostnames to obtain a certificate for
       * @param algorithm - Optional algorithm preference (e.g. Ed25519)
       */
      getSslCertificate: <E extends Effects>(
        effects: E,
        hostnames: string[],
        algorithm?: T.Algorithm,
      ) => new GetSslCertificate(effects, { hostnames, algorithm }),
      /** Retrieve the manifest of any installed service package by its ID */
      getServiceManifest,
      /**
       * Built-in health check functions for use in `ready.fn`.
       *
       * These cover the most common readiness patterns. For custom logic,
       * write your own async function returning a {@link HealthCheckResult}.
       */
      healthCheck: {
        /** Check whether a TCP/UDP port is bound and listening (reads /proc/net) */
        checkPortListening,
        /** Check whether a URL is reachable via HTTP fetch */
        checkWebUrl,
        /** Run a command in a subcontainer and succeed on exit code 0 */
        runHealthScript,
      },
      /** Common utility patterns (e.g. hostname regex, port validators) */
      patterns,
      /**
       * @description Use this function to list every Action offered by the service. Actions will be displayed in the provided order.
       *
       *   By convention, each Action should receive its own file in the "actions" directory.
       * @example
       * 
       * ```
        import { sdk } from '../sdk'
        import { config } from './config'
        import { nameToLogs } from './nameToLogs'

        export const actions = sdk.Actions.of().addAction(config).addAction(nameToLogs)
       * ```
       */
      Actions,
      /**
       * @description Use this function to determine which volumes are backed up when a user creates a backup, including advanced options.
       * @example
       * In this example, we back up the entire "main" volume and nothing else.
       *
       * ```
        import { sdk } from './sdk'

        export const { createBackup, restoreBackup } = sdk.setupBackups(
          async ({ effects }) => sdk.Backups.volumes('main'),
        )
       * ```
       * @example
       * In this example, we back up the "main" volume, but exclude hypothetical directory "excludedDir".
       *
       * ```
        import { sdk } from './sdk'

        export const { createBackup, restoreBackup } = sdk.setupBackups(async () =>
          sdk.Backups.volumes('main').setOptions({
            exclude: ['excludedDir'],
          }),
        )
       * ```
       */
      setupBackups: (options: SetupBackupsParams<Manifest>) =>
        setupBackups<Manifest>(options),
      /**
       * @description Use this function to set dependency information.
       * @example
       * In this example, we create a dependency on Hello World >=1.0.0:0, where Hello World must be running and passing its "primary" health check.
       *
       * ```
        export const setDependencies = sdk.setupDependencies(
          async ({ effects }) => {
            return {
              'hello-world': {
                kind: 'running',
                versionRange: '>=1.0.0',
                healthChecks: ['primary'],
              },
            }
          },
        )
       * ```
       */
      setupDependencies: <const R extends CurrentDependenciesResult<Manifest>>(
        fn: (options: {
          effects: T.Effects
        }) => Promise<R & ValidateVersionRanges<R>>,
      ) => setupDependencies<Manifest, R>(fn),
      /**
       * @description Use this function to create an InitScript that runs every time the service initializes (install, update, restore, rebuild, and server bootup)
       */
      setupOnInit,
      /**
       * @description Use this function to create an UninitScript that runs every time the service uninitializes (update, uninstall, and server shutdown)
       */
      setupOnUninit,
      /**
       * @description Use this function to setup what happens when the service initializes.
       *  
       *    This happens when the server boots, or a service is installed, updated, or restored
       * 
       *    Not every init script does something on every initialization. For example, versions only does something on install or update
       * 
       *    These scripts are run in the order they are supplied
       * @example
       *
       * ```
        export const init = sdk.setupInit(
          restoreInit,
          versions,
          setDependencies,
          setInterfaces,
          actions,
          postInstall,
        )
       * ```
       */
      setupInit: setupInit,
      /**
       * @description Use this function to setup what happens when the service uninitializes.
       *  
       *    This happens when the server shuts down, or a service is uninstalled or updated
       * 
       *    Not every uninit script does something on every uninitialization. For example, versions only does something on uninstall or update
       * 
       *    These scripts are run in the order they are supplied
       * @example
       *
       * ```
        export const uninit = sdk.setupUninit(
          versions,
        )
       * ```
       */
      setupUninit: setupUninit,
      /**
       * @description Use this function to determine how this service will be hosted and served. The function executes on service install, service update, and inputSpec save.
       * @param inputSpec - The inputSpec spec of this service as exported from /inputSpec/spec.
       * @param fn - an async function that returns an array of interface receipts. The function always has access to `effects`; it has access to `input` only after inputSpec save, otherwise `input` will be null.
       * @example
       * In this example, we create two UIs from one multi-host, and one API from another multi-host.
       *
       * ```
        export const setInterfaces = sdk.setupInterfaces(
          async ({ effects }) => {
            // ** UI multi-host **
            const uiMulti = sdk.MultiHost.of(effects, 'ui-multi')
            const uiMultiOrigin = await uiMulti.bindPort(80, {
              protocol: 'http',
            })
            // Primary UI
            const primaryUi = sdk.createInterface(effects, {
              name: 'Primary UI',
              id: 'primary-ui',
              description: 'The primary web app for this service.',
              type: 'ui',
              masked: false,
              schemeOverride: null,
              username: null,
              path: '',
              query: {},
            })
            // Admin UI
            const adminUi = sdk.createInterface(effects, {
              name: 'Admin UI',
              id: 'admin-ui',
              description: 'The admin web app for this service.',
              type: 'ui',
              masked: false,
              schemeOverride: null,
              username: null,
              path: '/admin',
              query: {},
            })
            // UI receipt
            const uiReceipt = await uiMultiOrigin.export([primaryUi, adminUi])
       
            // ** API multi-host **
            const apiMulti = sdk.MultiHost.of(effects, 'api-multi')
            const apiMultiOrigin = await apiMulti.bindPort(5959, {
              protocol: 'http',
            })
            // API
            const api = sdk.createInterface(effects, {
              name: 'Admin API',
              id: 'api',
              description: 'The advanced API for this service.',
              type: 'api',
              masked: false,
              schemeOverride: null,
              username: null,
              path: '',
              query: {},
            })
            // API receipt
            const apiReceipt = await apiMultiOrigin.export([api])
       
            // ** Return receipts **
            return [uiReceipt, apiReceipt]
          },
        )
       * ```
       */
      setupInterfaces: setupServiceInterfaces,
      /**
       * Define the main entrypoint for the service. The provided function should
       * configure and return a `Daemons` instance describing all long-running processes.
       * @param fn - Async function that receives `effects` and returns a `Daemons` instance
       */
      setupMain: (
        fn: (o: { effects: Effects }) => Promise<Daemons<Manifest, any>>,
      ) => setupMain<Manifest>(fn),
      /**
       * Built-in trigger strategies that control how often a health check polls.
       *
       * Pass any of these to the `trigger` field on a {@link Ready} config.
       * If omitted, health checks use `defaultTrigger` (1 s while pending, 30 s
       * once healthy).
       */
      trigger: {
        /**
         * The default trigger: 1 s while the daemon is still pending
         * (`starting`/`waiting`/`failure`), then 30 s once the first
         * non-pending result (`success`, `loading`, `disabled`) is observed.
         */
        defaultTrigger,
        /** Fixed interval between checks. Use for a simple, constant polling rate. */
        cooldownTrigger,
        /**
         * Per-status polling intervals. Map each health status (`success`,
         * `loading`, `starting`, `failure`, etc.) to a cooldown in
         * milliseconds. Unspecified statuses fall back to 30 s.
         */
        statusTrigger,
      },
      Mounts: {
        /**
         * Create an empty Mounts builder for declaring volume, asset, dependency, and backup mounts.
         * @returns A new Mounts instance with no mounts configured
         */
        of: Mounts.of<Manifest>,
      },
      Backups: {
        /**
         * Create a Backups configuration that backs up entire volumes by name.
         * @param volumeNames - Volume IDs from the manifest to include in backups
         */
        ofVolumes: Backups.ofVolumes<Manifest>,
        /**
         * Create a Backups configuration from explicit sync path pairs.
         * @param syncs - Array of `{ dataPath, backupPath }` objects
         */
        ofSyncs: Backups.ofSyncs<Manifest>,
        /**
         * Create a Backups configuration with custom rsync options (e.g. exclude patterns).
         * @param options - Partial sync options to override defaults
         */
        withOptions: Backups.withOptions<Manifest>,
        /**
         * Create a Backups configuration that uses pg_dump/pg_restore instead of
         * rsyncing the raw PostgreSQL data directory. Chain `.addVolume()` to include
         * additional volumes in the backup.
         */
        withPgDump: Backups.withPgDump<Manifest>,
        /**
         * Create a Backups configuration that uses mysqldump/mysql instead of
         * rsyncing the raw MySQL/MariaDB data directory. Chain `.addVolume()` to
         * include additional volumes in the backup.
         */
        withMysqlDump: Backups.withMysqlDump<Manifest>,
      },
      InputSpec: {
        /**
         * @description Use this function to define the inputSpec specification that will ultimately present to the user as validated form inputs.
         *
         *   Most form controls are supported, including text, textarea, number, toggle, select, multiselect, list, color, datetime, object (sub form), and union (conditional sub form).
         * @example
         * In this example, we define a inputSpec form with two value: name and makePublic.
         *
         * ```
          import { sdk } from '../sdk'
          const { InputSpec, Value } = sdk
         
          export const inputSpecSpec = InputSpec.of({
            name: Value.text({
              name: 'Name',
              description:
                'When you launch the Hello World UI, it will display "Hello [Name]"',
              required: true,
              default: 'World'
            }),
            makePublic: Value.toggle({
              name: 'Make Public',
              description: 'Whether or not to expose the service to the network',
              default: false,
            }),
          })
         * ```
         */
        of: <Spec extends Record<string, Value<any>>>(spec: Spec) =>
          InputSpec.of<Spec>(spec),
      },
      Daemon: {
        /**
         * Create a single Daemon that wraps a long-running process with automatic restart logic.
         * Returns a curried function: call with `(effects, subcontainer, exec)`.
         */
        get of() {
          return Daemon.of<Manifest>()
        },
      },
      Daemons: {
        /**
         * Create a new Daemons builder for defining the service's daemon topology.
         * Chain `.addDaemon()` calls to register each long-running process.
         * @param effects - The effects context
         */
        of(effects: Effects) {
          return Daemons.of<Manifest>({ effects })
        },
        /**
         * Build a reactive `main` entrypoint that reconciles its daemon set
         * against a `Daemons` chain on every `effects.constRetry` trigger.
         * See {@link Daemons.dynamic} for diff semantics and the rule that
         * `fn`'s subcontainers must be lazy (`sdk.SubContainer.of(...)`).
         *
         * @param fn Async builder invoked on startup and on every constRetry
         */
        dynamic(
          fn: (o: {
            effects: Effects
          }) => Promise<Daemons<Manifest, any>> | Daemons<Manifest, any>,
        ) {
          return Daemons.dynamic<Manifest>(fn)
        },
      },
      SubContainer: {
        /**
         * @description Create a new SubContainer
         * @param effects
         * @param image - what container image to use
         * @param mounts - what to mount to the subcontainer
         * @param name - a name to use to refer to the subcontainer for debugging purposes
         */
        of(
          effects: Effects,
          image: {
            imageId: T.ImageId & keyof Manifest['images']
            sharedRun?: boolean
          },
          mounts: Mounts<Manifest> | null,
          name: string,
        ): SubContainerLazy<Manifest, Effects> {
          return SubContainer.of<Manifest, Effects>(
            effects,
            image,
            mounts,
            name,
          )
        },
        /**
         * Create an eager SubContainer, materializing its backing
         * filesystem immediately. Use when you need synchronous access to
         * `rootfs` / `guid` / `subpath()` before running any methods, or
         * want `createFs` failures to surface here instead of at first
         * method call.
         *
         * **Do not pass an eager SubContainer to `Daemons.dynamic`'s `fn`** —
         * see {@link SubContainer.eager}.
         */
        eager(
          effects: Effects,
          image: {
            imageId: T.ImageId & keyof Manifest['images']
            sharedRun?: boolean
          },
          mounts: Mounts<Manifest> | null,
          name: string,
        ): Promise<SubContainerEager<Manifest, Effects>> {
          return SubContainer.eager<Manifest, Effects>(
            effects,
            image,
            mounts,
            name,
          )
        },
        /**
         * Run a function with an ephemeral, eager SubContainer. The
         * container is created before `fn` runs and destroyed in a
         * `finally` block after.
         */
        withTemp<T>(
          effects: Effects,
          image: {
            imageId: T.ImageId & keyof Manifest['images']
            sharedRun?: boolean
          },
          mounts: Mounts<Manifest> | null,
          name: string,
          fn: (
            subContainer: SubContainerEager<Manifest, Effects>,
          ) => Promise<T>,
        ): Promise<T> {
          return SubContainer.withTemp<Manifest, T, Effects>(
            effects,
            image,
            mounts,
            name,
            fn,
          )
        },
      },
      List,
      Value,
      Variants,
      plugin: {
        url: this.ifPluginEnabled('url-v0' as const, {
          register: (
            effects: T.Effects,
            options: {
              tableAction: ActionInfo<
                T.ActionId,
                {
                  urlPluginMetadata: {
                    packageId: T.PackageId
                    interfaceId: T.ServiceInterfaceId
                    hostId: T.HostId
                    internalPort: number
                  }
                }
              >
            },
          ) =>
            effects.plugin.url.register({
              tableAction: options.tableAction.id,
            }),
          exportUrl: (
            effects: T.Effects,
            options: {
              hostnameInfo: T.PluginHostnameInfo
              removeAction: ActionInfo<
                T.ActionId,
                {
                  urlPluginMetadata: T.PluginHostnameInfo & {
                    interfaceId: T.ServiceInterfaceId
                  }
                }
              > | null
              overflowActions: ActionInfo<
                T.ActionId,
                {
                  urlPluginMetadata: T.PluginHostnameInfo & {
                    interfaceId: T.ServiceInterfaceId
                  }
                }
              >[]
            },
          ) =>
            effects.plugin.url.exportUrl({
              hostnameInfo: options.hostnameInfo,
              removeAction: options.removeAction?.id ?? null,
              overflowActions: options.overflowActions.map(a => a.id),
            }),
          setupExportedUrls, // similar to setupInterfaces
        }),
      },
    }
  }
}

/**
 * Run a one-shot command inside a temporary subcontainer.
 *
 * Creates a subcontainer, executes the command, and destroys the subcontainer when finished.
 * Throws an {@link ExitError} if the command exits with a non-zero code or signal.
 *
 * @param effects - The effects context
 * @param image - The container image to use
 * @param command - The command to execute (string array or UseEntrypoint)
 * @param options - Mount and command options
 * @param name - Optional human-readable name for debugging
 * @returns The stdout and stderr output of the command
 */
export async function runCommand<Manifest extends T.SDKManifest>(
  effects: Effects,
  image: { imageId: keyof Manifest['images'] & T.ImageId; sharedRun?: boolean },
  command: T.CommandType,
  options: CommandOptions & {
    mounts: Mounts<Manifest> | null
  },
  name?: string,
): Promise<{ stdout: string | Buffer; stderr: string | Buffer }> {
  let commands: string[]
  if (T.isUseEntrypoint(command)) {
    const imageMeta: T.ImageMetadata = await fs
      .readFile(`/media/startos/images/${image.imageId}.json`, {
        encoding: 'utf8',
      })
      .catch(() => '{}')
      .then(JSON.parse)
    commands = imageMeta.entrypoint ?? []
    commands = commands.concat(...(command.overridCmd ?? imageMeta.cmd ?? []))
  } else commands = splitCommand(command)
  return SubContainer.withTemp<
    Manifest,
    { stdout: string | Buffer; stderr: string | Buffer }
  >(
    effects,
    image,
    options.mounts,
    name ||
      commands
        .map(c => {
          if (c.includes(' ')) {
            return `"${c.replace(/"/g, `\"`)}"`
          } else {
            return c
          }
        })
        .join(' '),
    async subcontainer => {
      const res = await subcontainer.exec(commands)
      if (res.exitCode || res.exitSignal) {
        throw new ExitError(commands[0], res)
      } else {
        return res
      }
    },
  )
}
