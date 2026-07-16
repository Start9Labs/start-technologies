import { i18n } from './i18n'
import { sdk } from './sdk'
import { uiPort } from './utils'

export const main = sdk.setupMain(async ({ effects }) => {
  /**
   * ======================== Setup (optional) ========================
   *
   * Fetch any resources or run any preliminary commands here.
   */
  console.info(i18n('Starting {{name}}!'))

  /**
   * ======================== Daemons ========================
   *
   * Define one or more daemons that make up the service runtime. Each daemon
   * declares a `ready` health check, run on every polling interval, that reports
   * its state to the user.
   *
   * The ids here ('example-daemon', 'example-image', 'example-volume',
   * 'example-subcontainer') are arbitrary — rename them to suit your service.
   * 'example-image' must match an image key in startos/manifest/index.ts, and
   * 'example-volume' must match an entry in the manifest `volumes` array.
   */
  return sdk.Daemons.of(effects).addDaemon('example-daemon', {
    subcontainer: sdk.SubContainer.of(
      effects,
      { imageId: 'example-image' },
      sdk.Mounts.of().mountVolume({
        volumeId: 'example-volume',
        subpath: null,
        mountpoint: '/data',
        readonly: false,
      }),
      'example-subcontainer',
    ),
    exec: { command: ['hello-world'] },
    // Health check, run on each polling interval. `checkPortListening` reports
    // ready once the daemon binds `uiPort`; expose that port from an interface
    // (see interfaces.ts) so users can reach it. For other services swap in
    // `checkWebUrl` or `runHealthScript` — see the Health Checks guide.
    ready: {
      display: i18n('Web Interface'),
      fn: () =>
        sdk.healthCheck.checkPortListening(effects, uiPort, {
          successMessage: i18n('The web interface is ready'),
          errorMessage: i18n('The web interface is not ready'),
        }),
    },
    requires: [],
  })
})
