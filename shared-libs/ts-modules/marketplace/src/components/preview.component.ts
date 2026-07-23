import { CommonModule } from '@angular/common'
import {
  Component,
  computed,
  inject,
  input,
  signal,
  TemplateRef,
} from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { Router } from '@angular/router'
import {
  DialogService,
  EmptyPipe,
  Exver,
  i18nPipe,
  MARKDOWN,
} from '@start9labs/shared'
import { TuiLoader, TuiNotification, TuiTitle } from '@taiga-ui/core'
import { TuiAvatar, TuiFade } from '@taiga-ui/kit'
import {
  tuiCardOptionsProvider,
  TuiHeader,
  tuiHeaderOptionsProvider,
} from '@taiga-ui/layout'
import {
  BehaviorSubject,
  combineLatest,
  filter,
  map,
  shareReplay,
  startWith,
  switchMap,
  take,
  tap,
} from 'rxjs'

import { AbstractMarketplaceService } from '../services/abstract-marketplace.service'
import { MarketplacePkg } from '../types'
import { MarketplaceAboutComponent } from './about.component'
import { MarketplaceDependenciesComponent } from './dependencies.component'
import { MarketplaceFlavorsComponent } from './flavors.component'
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
    } @else if (pkg$ | async; as pkg) {
      <header tuiHeader>
        <span tuiAvatar [round]="false">
          <img
            alt=""
            [src]="pkg.icon || 'assets/img/service-icons/fallback.png'"
          />
        </span>
        <span tuiTitle tuiFade>{{ pkg.title }}</span>
        <span tuiAccessories><ng-content /></span>
      </header>
      <ng-container
        *ngTemplateOutlet="controls(); context: { $implicit: pkg }"
      />
      <marketplace-about
        [pkg]="pkg"
        [versions]="versions$ | async"
        (static)="onStatic()"
        (onVersion)="selectedVersion$.next($event)"
      />
      <marketplace-release-notes [pkg]="pkg" />
      @if (flavors$ | async; as flavors) {
        <marketplace-flavors [pkgs]="flavors" />
      }
      @if (!(pkg.dependencyMetadata | empty)) {
        <marketplace-dependencies [pkg]="pkg" />
      }
      <marketplace-links [pkg]="pkg" />
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
    TuiLoader,
    TuiNotification,
    TuiTitle,
    TuiHeader,
    TuiAvatar,
    TuiFade,
    MarketplaceDependenciesComponent,
    MarketplaceLinksComponent,
    MarketplaceFlavorsComponent,
    MarketplaceAboutComponent,
    MarketplaceReleaseNotesComponent,
    i18nPipe,
  ],
})
export class MarketplacePreviewComponent {
  private readonly dialog = inject(DialogService)
  private readonly exver = inject(Exver)
  private readonly router = inject(Router)
  private readonly marketplaceService = inject(AbstractMarketplaceService)

  readonly pkgId = input.required<string>()

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
    this.params()?.get('requirePkg') === this.pkgId()
      ? this.params()?.get('requireRange')
      : null,
  )

  readonly selectedVersion$ = new BehaviorSubject<string | null>(null)

  readonly noMatch = signal(false)

  readonly pkg$ = combineLatest([this.selectedVersion$, this.flavor$]).pipe(
    switchMap(([version, flavor]) =>
      this.marketplaceService.getPackage$(this.pkgId(), version, flavor).pipe(
        tap(pkg => {
          if (!pkg || version) return
          const range = this.targetRange()
          if (!range || this.satisfiesWithAliases(pkg, range)) return
          const flv = this.exver.getFlavor(pkg.version)
          const candidate = Object.keys(pkg.otherVersions || {})
            .filter(
              v =>
                this.exver.getFlavor(v) === flv &&
                this.exver.satisfies(v, range),
            )
            .sort((a, b) => -1 * (this.exver.compareExver(a, b) || 0))[0]
          if (candidate) {
            this.selectedVersion$.next(candidate)
          } else {
            this.noMatch.set(true)
          }
        }),
        filter(pkg => {
          const range = this.targetRange()

          return (
            !!pkg &&
            (!!version || !range || this.satisfiesWithAliases(pkg, range))
          )
        }),
        startWith<MarketplacePkg | null>(null),
      ),
    ),
    shareReplay({ bufferSize: 1, refCount: false }),
  )

  private satisfiesWithAliases(pkg: MarketplacePkg, range: string): boolean {
    return (
      this.exver.satisfies(pkg.version, range) ||
      (pkg.satisfies || []).some(v => this.exver.satisfies(v, range))
    )
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

  readonly versions$ = this.flavor$.pipe(
    switchMap(flavor =>
      this.marketplaceService.getPackage$(this.pkgId(), null, flavor).pipe(
        filter(Boolean),
        map(pkg => {
          const range = this.targetRange()
          const others = Object.keys(pkg.otherVersions).filter(
            v => this.exver.getFlavor(v) === flavor,
          )
          return [pkg.version, ...others]
            .filter(v =>
              v === pkg.version
                ? !range || this.satisfiesWithAliases(pkg, range)
                : !range || this.exver.satisfies(v, range),
            )
            .sort((a, b) => -1 * (this.exver.compareExver(a, b) || 0))
        }),
      ),
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
