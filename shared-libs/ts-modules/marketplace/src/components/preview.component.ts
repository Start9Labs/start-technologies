import { CommonModule } from '@angular/common'
import {
  Component,
  computed,
  inject,
  input,
  signal,
  TemplateRef,
} from '@angular/core'
import { toObservable, toSignal } from '@angular/core/rxjs-interop'
import { Router } from '@angular/router'
import {
  DialogService,
  EmptyPipe,
  Exver,
  i18nPipe,
  MARKDOWN,
} from '@start9labs/shared'
import { TuiButton, TuiLoader, TuiNotification, TuiTitle } from '@taiga-ui/core'
import { TuiAvatar, TuiFade, TuiSkeleton } from '@taiga-ui/kit'
import {
  tuiCardOptionsProvider,
  TuiHeader,
  tuiHeaderOptionsProvider,
} from '@taiga-ui/layout'
import {
  combineLatest,
  distinctUntilChanged,
  filter,
  firstValueFrom,
  map,
  shareReplay,
  startWith,
  switchMap,
  take,
  tap,
} from 'rxjs'

import { AbstractMarketplaceService } from '../services/abstract-marketplace.service'
import { MarketplaceListing, MarketplacePkg } from '../types'
import {
  compatibleVersions,
  satisfiesWithAliases,
} from '../util/compatible-versions'
import { MarketplaceAboutComponent } from './about.component'
import { MarketplaceDependenciesComponent } from './dependencies.component'
import { MarketplaceFlavorsComponent } from './flavors.component'
import { MarketplaceIconDirective } from './icon.directive'
import { MarketplaceLinksComponent } from './links.component'
import { MarketplaceReleaseNotesComponent } from './release-notes.component'

@Component({
  selector: 'marketplace-preview',
  template: `
    @if (noMatch()) {
      <div tuiNotification appearance="negative">
        <div tuiTitle>
          {{ 'No compatible version available' | i18n }}
          <div tuiSubtitle>{{ 'Requires' | i18n }}: {{ targetRange() }}</div>
        </div>
      </div>
      @if (flavors$ | async; as flavors) {
        <marketplace-flavors [pkgs]="flavors" />
      }
    } @else if (pkg() || placeholder(); as shown) {
      <header tuiHeader>
        <span tuiAvatar [round]="false" [tuiSkeleton]="icon.loading()">
          <img #icon="marketplaceIcon" [marketplaceIcon]="shown" />
        </span>
        <span tuiTitle tuiFade>{{ shown.title }}</span>
        <span tuiAccessories><ng-content /></span>
      </header>
      @if (flavorFallback) {
        <div tuiNotification appearance="info">
          <div tuiTitle>
            {{ 'This alternative implementation meets the requirement' | i18n }}
            <div tuiSubtitle>{{ 'Requires' | i18n }}: {{ targetRange() }}</div>
          </div>
        </div>
      }
      @if (pkg(); as pkg) {
        <ng-container
          *ngTemplateOutlet="controls(); context: { $implicit: pkg }"
        />
      } @else if (controls()) {
        <div class="g-buttons">
          <button tuiButton type="button" [tuiSkeleton]="true">
            {{ 'Install' | i18n }}
          </button>
        </div>
      }
      <marketplace-about
        [pkg]="shown"
        [loading]="!pkg()"
        [versions]="versions$ | async"
        (static)="onStatic()"
        (onVersion)="selectedVersion.set($event)"
      />
      <marketplace-release-notes [pkg]="shown" />
      @if (flavors$ | async; as flavors) {
        <marketplace-flavors [pkgs]="flavors" />
      }
      @if (pkg(); as pkg) {
        @if (!(pkg.dependencyMetadata | empty)) {
          <marketplace-dependencies [pkg]="pkg" />
        }
        <marketplace-links [pkg]="pkg" />
      }
    } @else {
      <tui-loader [textContent]="'Loading' | i18n" />
    }
  `,
  styles: `
    :host {
      display: grid;
      gap: 1rem;
    }

    header {
      gap: 1rem;
      align-items: center;
      white-space: nowrap;
      overflow: hidden;
    }
  `,
  providers: [
    tuiHeaderOptionsProvider({ size: 'h6' }),
    tuiCardOptionsProvider({ appearance: 'secondary-grayscale' }),
  ],
  imports: [
    CommonModule,
    EmptyPipe,
    TuiButton,
    TuiLoader,
    TuiNotification,
    TuiTitle,
    TuiHeader,
    TuiAvatar,
    TuiSkeleton,
    TuiFade,
    MarketplaceDependenciesComponent,
    MarketplaceLinksComponent,
    MarketplaceFlavorsComponent,
    MarketplaceAboutComponent,
    MarketplaceReleaseNotesComponent,
    MarketplaceIconDirective,
    i18nPipe,
  ],
})
export class MarketplacePreviewComponent {
  private readonly dialog = inject(DialogService)
  private readonly exver = inject(Exver)
  private readonly router = inject(Router)
  private readonly marketplaceService = inject(AbstractMarketplaceService)

  readonly pkgId = input.required<string>()

  /** The selected release's listing, shown while its full details load. */
  readonly listing = input<MarketplaceListing>()

  /** Action buttons for the resolved package (install/update), rendered with
   * the resolved `MarketplacePkg` as `$implicit`. The brochure omits it. */
  readonly controls = input<TemplateRef<{ $implicit: MarketplacePkg }>>()

  private readonly params$ = this.router.routerState.root.queryParamMap.pipe(
    take(1),
    shareReplay(1),
  )

  private readonly params = toSignal(this.params$)
  private readonly flavor$ = this.params$.pipe(map(p => p.get('flavor')))

  readonly targetRange = computed(() =>
    this.params()?.get('requirePkg') &&
    (this.params()?.get('requirePkg') === this.pkgId() ||
      this.params()?.get('provides') === this.params()?.get('requirePkg'))
      ? this.params()?.get('requireRange')
      : null,
  )

  private readonly provides = computed(() => this.params()?.get('requirePkg'))

  readonly selectedVersion = signal<string | null>(null)

  readonly noMatch = signal(false)

  readonly flavorFallback =
    !!this.router.lastSuccessfulNavigation()?.extras.state?.['flavorFallback']

  readonly pkg$ = combineLatest([
    toObservable(this.selectedVersion),
    toObservable(this.listing).pipe(
      map(listing => listing?.version ?? null),
      distinctUntilChanged(),
    ),
    this.flavor$,
  ]).pipe(
    switchMap(([version, listedVersion, flavor]) =>
      this.marketplaceService
        .getPackage$(this.pkgId(), version ?? listedVersion, flavor)
        .pipe(
          tap(pkg => {
            if (!pkg || version) return
            const range = this.targetRange()
            if (
              !range ||
              satisfiesWithAliases(this.exver, pkg, range, this.provides())
            )
              return
            const candidate = compatibleVersions(
              this.exver,
              pkg,
              range,
              this.provides(),
            )[0]
            if (candidate) {
              this.selectedVersion.set(candidate)
            } else {
              this.selectSatisfyingFlavor(pkg.flavor, range)
            }
          }),
          filter(pkg => {
            const range = this.targetRange()

            return (
              !!pkg &&
              (!range ||
                satisfiesWithAliases(this.exver, pkg, range, this.provides()))
            )
          }),
          startWith<MarketplacePkg | null>(null),
        ),
    ),
    shareReplay({ bufferSize: 1, refCount: true }),
  )

  protected readonly pkg = toSignal(this.pkg$, { initialValue: null })

  protected readonly placeholder = computed(() => {
    const listing = this.listing()
    const range = this.targetRange()
    return !this.selectedVersion() &&
      listing &&
      (!range ||
        satisfiesWithAliases(this.exver, listing, range, this.provides()))
      ? listing
      : null
  })

  private async selectSatisfyingFlavor(current: string | null, range: string) {
    const { packages } = await firstValueFrom(
      this.marketplaceService.currentRegistry$,
    )
    const flavors = packages.filter(
      p => p.id === this.pkgId() && p.flavor !== current,
    )
    const resolved = await Promise.all(
      flavors.map(p =>
        firstValueFrom(
          this.marketplaceService.getPackage$(p.id, p.version, p.flavor),
        ),
      ),
    )
    const match = resolved.find(
      p =>
        !!p &&
        !!compatibleVersions(this.exver, p, range, this.provides()).length,
    )

    if (match) {
      this.router.navigate([], {
        queryParams: { flavor: match.flavor },
        queryParamsHandling: 'merge',
        replaceUrl: true,
        state: { flavorFallback: true },
      })
    } else {
      this.noMatch.set(true)
    }
  }

  readonly flavors$ = this.flavor$.pipe(
    switchMap(current =>
      this.marketplaceService.currentRegistry$.pipe(
        map(({ packages }) =>
          packages.filter(
            ({ id, flavor }) => id === this.pkgId() && flavor !== current,
          ),
        ),
        filter(p => !!p.length),
      ),
    ),
  )

  readonly versions$ = this.pkg$.pipe(
    filter(Boolean),
    map(pkg =>
      compatibleVersions(this.exver, pkg, this.targetRange(), this.provides()),
    ),
  )

  onStatic() {
    this.dialog
      .openComponent(MARKDOWN, {
        label: 'License',
        size: 'l',
        data: this.pkg$.pipe(
          filter(Boolean),
          switchMap(pkg =>
            this.marketplaceService.fetchStatic$(pkg, 'LICENSE.md'),
          ),
        ),
      })
      .subscribe()
  }
}
