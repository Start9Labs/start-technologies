import { CommonModule, TitleCasePipe } from '@angular/common'
import { Component, computed, inject, input } from '@angular/core'
import { toObservable, toSignal } from '@angular/core/rxjs-interop'
import { Router } from '@angular/router'
import { MarketplacePkg } from '@start9labs/marketplace'
import {
  Exver,
  ExverComparesPipe,
  i18nPipe,
  sameUrl,
  TaskService,
} from '@start9labs/shared'
import { TuiButton } from '@taiga-ui/core'
import { PatchDB } from 'patch-db-client'
import { firstValueFrom, switchMap } from 'rxjs'
import { ToManifestPipe } from 'src/app/routes/portal/pipes/to-manifest'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { MarketplaceService } from 'src/app/services/marketplace.service'
import { DataModel } from 'src/app/services/patch-db/data-model'
import { dryUpdate } from 'src/app/utils/dry-update'
import { getAllPackages, getManifest } from 'src/app/utils/get-package-data'
import { hasCurrentDeps } from 'src/app/utils/has-deps'

import { MarketplaceAlertsService } from '../services/alerts.service'

type KEYS = 'id' | 'version' | 'flavor' | 'satisfies' | 'preDownloadAlert'

@Component({
  selector: 'marketplace-controls',
  template: `
    @if (sameFlavor() && localPkg(); as local) {
      @if (local.stateInfo.state === 'installed') {
        @switch ((local | toManifest).version | compareExver: pkg().version) {
          @case (1) {
            <button
              tuiButton
              type="button"
              appearance="warning"
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
        (click)="tryInstall()"
      >
        {{ (sameFlavor() ? 'Install' : 'Switch') | i18n }}
      </button>
    }
  `,
  styles: `
    :host {
      display: flex;
      gap: 0.5rem;
    }
  `,
  imports: [
    CommonModule,
    ExverComparesPipe,
    TuiButton,
    ToManifestPipe,
    i18nPipe,
    TitleCasePipe,
  ],
})
export class MarketplaceControlsComponent {
  private readonly alerts = inject(MarketplaceAlertsService)
  private readonly patch = inject<PatchDB<DataModel>>(PatchDB)
  private readonly tasks = inject(TaskService)
  private readonly exver = inject(Exver)
  private readonly router = inject(Router)
  private readonly marketplace = inject(MarketplaceService)
  private readonly api = inject(ApiService)

  readonly pkg = input.required<Pick<MarketplacePkg, KEYS>>()

  // only present if side loading
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

  async tryInstall() {
    const localPkg = this.localPkg()
    if (
      !this.file() &&
      !(await this.alerts.alertPreDownload(
        this.pkg().preDownloadAlert,
        localPkg ? getManifest(localPkg).version : null,
      ))
    ) {
      return
    }

    const currentUrl = this.file()
      ? null
      : await firstValueFrom(this.marketplace.currentRegistryUrl$)
    const originalUrl = localPkg?.registry || null

    if (!localPkg) {
      this.installOrUpload(currentUrl)
      return
    }

    if (
      this.sameFlavor() &&
      currentUrl &&
      !sameUrl(currentUrl, originalUrl) &&
      !(await this.alerts.alertMarketplace(currentUrl, originalUrl))
    ) {
      return
    }

    const { id, version } = getManifest(localPkg)

    if (
      hasCurrentDeps(id, await getAllPackages(this.patch)) &&
      this.exver.compareExver(version, this.pkg().version) !== 0
    ) {
      this.dryInstall(currentUrl)
    } else {
      this.installOrUpload(currentUrl)
    }
  }

  async showService() {
    this.router.navigate(['services', this.pkg().id])
  }

  private async dryInstall(url: string | null) {
    const { id, version, satisfies } = this.pkg()
    const packages = await getAllPackages(this.patch)
    const breakages = dryUpdate(
      { id, version, satisfies: satisfies || [] },
      packages,
      this.exver,
    )

    if (!breakages.length || (await this.alerts.alertBreakages(breakages))) {
      this.installOrUpload(url)
    }
  }

  private async installOrUpload(url: string | null) {
    if (this.file()) {
      await this.upload()
      this.router.navigate(['services'])
    } else if (url) {
      await this.install(url)
    }
  }

  private async install(url: string) {
    const { id, version } = this.pkg()
    this.tasks.run(
      async () => await this.marketplace.installPackage(id, version, url),
      'Beginning install',
    )
  }

  private async upload() {
    const file = this.file()
    if (!file) throw new Error('no file detected')

    this.tasks.run(async () => {
      const res = await this.api.sideloadPackage()
      this.api.uploadFile(res.upload, file).catch(console.error)
    }, 'Starting upload')
  }
}
