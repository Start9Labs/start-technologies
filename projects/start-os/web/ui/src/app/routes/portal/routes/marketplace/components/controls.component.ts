import { CommonModule, TitleCasePipe } from '@angular/common'
import { Component, computed, inject, input } from '@angular/core'
import { toObservable, toSignal } from '@angular/core/rxjs-interop'
import { Router } from '@angular/router'
import {
  MarketplacePkg,
  matchingTrustedKeys,
  packageSigners,
} from '@start9labs/marketplace'
import {
  DialogService,
  ErrorService,
  Exver,
  ExverComparesPipe,
  i18nPipe,
  sameUrl,
  TaskService,
} from '@start9labs/shared'
import { TuiButton, TuiNotification } from '@taiga-ui/core'
import { PatchDB } from 'patch-db-client'
import { defaultIfEmpty, firstValueFrom, switchMap } from 'rxjs'
import { ToManifestPipe } from 'src/app/routes/portal/pipes/to-manifest'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { MarketplaceService } from 'src/app/services/marketplace.service'
import { DataModel } from 'src/app/services/patch-db/data-model'
import { TrustedKeysService } from 'src/app/services/trusted-keys.service'
import { dryUpdate } from 'src/app/utils/dry-update'
import { getAllPackages, getManifest } from 'src/app/utils/get-package-data'
import { hasCurrentDeps } from 'src/app/utils/has-deps'
import { MarketplaceAlertsService } from '../services/alerts.service'

type KEYS =
  | 'id'
  | 'version'
  | 'flavor'
  | 'satisfies'
  | 'preDownloadAlert'
  | 's9pks'
  | 'signers'

@Component({
  selector: 'marketplace-controls',
  template: `
    @if (!trusted().length) {
      <div tuiNotification appearance="warning">
        {{ "This service package isn't signed by anyone you trust" | i18n }}
      </div>
    }
    <div class="g-buttons">
      @if (sameFlavor() && localPkg(); as local) {
        @if (local.stateInfo.state === 'installed') {
          @switch ((local | toManifest).version | compareExver: pkg().version) {
            @case (1) {
              <button
                tuiButton
                type="button"
                appearance="warning"
                [disabled]="blocked()"
                (click)="tryInstall()"
              >
                {{ 'Downgrade' | i18n }}
              </button>
            }
            @case (-1) {
              <button
                tuiButton
                type="button"
                appearance="primary"
                [disabled]="blocked()"
                (click)="tryInstall()"
              >
                {{ 'Update' | i18n }}
              </button>
            }
            @case (0) {
              <button
                tuiButton
                type="button"
                appearance="secondary-grayscale"
                [disabled]="blocked()"
                (click)="tryInstall()"
              >
                {{ 'Reinstall' | i18n }}
              </button>
            }
          }
        }
        <button
          tuiButton
          type="button"
          appearance="secondary-grayscale"
          (click)="showService()"
        >
          {{ 'View' | i18n }}
          {{ $any(local.stateInfo.state | titlecase) | i18n }}
        </button>
      } @else {
        <button
          tuiButton
          type="button"
          appearance="primary"
          [disabled]="blocked()"
          (click)="tryInstall()"
        >
          {{ (sameFlavor() ? 'Install' : 'Switch') | i18n }}
        </button>
      }
    </div>
  `,
  styles: `
    :host {
      display: grid;
      gap: 0.75rem;
    }
  `,
  imports: [
    CommonModule,
    ExverComparesPipe,
    TuiButton,
    TuiNotification,
    ToManifestPipe,
    i18nPipe,
    TitleCasePipe,
  ],
})
export class MarketplaceControlsComponent {
  private readonly alerts = inject(MarketplaceAlertsService)
  private readonly dialogs = inject(DialogService)
  private readonly trust = inject(TrustedKeysService)
  private readonly patch = inject<PatchDB<DataModel>>(PatchDB)
  private readonly tasks = inject(TaskService)
  private readonly error = inject(ErrorService)
  private readonly exver = inject(Exver)
  private readonly router = inject(Router)
  private readonly marketplace = inject(MarketplaceService)
  private readonly api = inject(ApiService)

  readonly pkg = input.required<Pick<MarketplacePkg, KEYS>>()
  readonly file = input<File>()

  readonly localPkg = toSignal(
    toObservable(this.pkg).pipe(
      switchMap(({ id }) => this.patch.watch$('packageData', id)),
    ),
  )
  readonly sameFlavor = computed(
    (pkg = this.localPkg()) =>
      !pkg ||
      this.exver.getFlavor(getManifest(pkg).version) === this.pkg().flavor,
  )
  protected readonly trusted = computed(() =>
    matchingTrustedKeys(
      packageSigners(this.pkg()),
      this.trust.keys() ?? {},
      'marketplace',
    ),
  )
  protected readonly blocked = computed(
    () => !this.file() && !this.trusted().length,
  )

  async tryInstall() {
    if (this.blocked()) return
    const trustOverride = !!this.file() && !this.trusted().length
    if (
      trustOverride &&
      !(await firstValueFrom(
        this.dialogs
          .openConfirm({
            label: 'Install untrusted software?',
            data: {
              content:
                'This service package is not signed by anyone you trust. Malicious software could steal private keys and passwords, compromise your server, or permanently destroy your data. Only continue if you trust the source and understand these risks.',
              yes: 'Install anyway',
              no: 'Cancel',
            },
          })
          .pipe(defaultIfEmpty(false)),
      ))
    )
      return

    const localPkg = this.localPkg()
    if (
      !this.file() &&
      !(await this.alerts.alertPreDownload(
        this.pkg().preDownloadAlert,
        localPkg ? getManifest(localPkg).version : null,
      ))
    )
      return

    const currentUrl = this.file()
      ? null
      : await firstValueFrom(this.marketplace.currentRegistryUrl$)
    const originalUrl = localPkg?.registry || null
    if (!localPkg) {
      await this.installOrUpload(currentUrl, trustOverride)
      return
    }
    if (
      this.sameFlavor() &&
      currentUrl &&
      !sameUrl(currentUrl, originalUrl) &&
      !(await this.alerts.alertMarketplace(currentUrl, originalUrl))
    )
      return

    const { id, version } = getManifest(localPkg)
    if (
      hasCurrentDeps(id, await getAllPackages(this.patch)) &&
      this.exver.compareExver(version, this.pkg().version) !== 0
    ) {
      await this.dryInstall(currentUrl, trustOverride)
    } else {
      await this.installOrUpload(currentUrl, trustOverride)
    }
  }

  async showService() {
    this.router.navigate(['services', this.pkg().id])
  }

  private async dryInstall(url: string | null, trustOverride: boolean) {
    const { id, version, satisfies } = this.pkg()
    const packages = await getAllPackages(this.patch)
    const breakages = dryUpdate(
      { id, version, satisfies: satisfies || [] },
      packages,
      this.exver,
    )
    if (!breakages.length || (await this.alerts.alertBreakages(breakages))) {
      await this.installOrUpload(url, trustOverride)
    }
  }

  private async installOrUpload(url: string | null, trustOverride: boolean) {
    if (this.file()) {
      if (await this.upload(trustOverride)) this.router.navigate(['services'])
    } else if (url) {
      const pkg = this.pkg()
      await this.tasks.run(
        async () => await this.marketplace.installPackage(pkg, url),
        'Beginning install',
      )
    }
  }

  private async upload(trust: boolean): Promise<boolean> {
    const file = this.file()
    if (!file) throw new Error('no file detected')
    return this.tasks.run(async () => {
      const res = await this.api.rpc.package.sideload({ trust })
      this.api
        .uploadFile(res.upload, file)
        .catch(e => this.error.handleError(e))
    }, 'Starting upload')
  }
}
