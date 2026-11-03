import { DatePipe } from '@angular/common'
import { Component, computed, inject, input, output } from '@angular/core'
import { CopyService, i18nPipe, LocalizePipe } from '@start9labs/shared'
import { TuiCell, TuiIcon, TuiTitle } from '@taiga-ui/core'
import { TuiCardLarge, TuiHeader } from '@taiga-ui/layout'
import { MarketplacePkgBase } from '../types'
import { MarketplaceTrust, packageSigners } from '../trust'
import { MarketplaceSignersComponent } from './signers.component'
import { MarketplaceVersionsComponent } from './versions.component'

@Component({
  selector: 'marketplace-about',
  template: `
    <section tuiCardLarge>
      @if ((versions()?.length || 0) > 1) {
        <button
          marketplaceVersions
          [version]="pkg().version"
          [versions]="versions() || []"
          (versionChange)="onVersion.emit($event)"
        ></button>
      } @else {
        <div tuiCell>
          <span tuiTitle>
            <span tuiSubtitle>{{ 'Version' | i18n }}</span>
            {{ pkg().version }}
          </span>
        </div>
      }
      @if (pkg().s9pks[0]?.[1]?.publishedAt; as published) {
        <div tuiCell>
          <span tuiTitle>
            <span tuiSubtitle>{{ 'Released' | i18n }}</span>
            {{ published | date: 'medium' }}
          </span>
        </div>
      }
      <div tuiCell>
        <span tuiTitle>
          <span tuiSubtitle>{{ 'SDK version' | i18n }}</span>
          {{ pkg().sdkVersion || 'Unknown' }}
        </span>
      </div>
      @if (pkg().gitHash; as gitHash) {
        <button type="button" tuiCell (click)="copyService.copy(gitHash)">
          <span tuiTitle>
            <span tuiSubtitle>{{ 'Git hash' | i18n }}</span>
            {{ gitHash }}
          </span>
          <tui-icon icon="@tui.copy" />
        </button>
      } @else {
        <div tuiCell>
          <span tuiTitle>
            <span tuiSubtitle>{{ 'Git hash' | i18n }}</span>
            {{ 'Unknown' | i18n }}
          </span>
        </div>
      }
      <button type="button" tuiCell (click)="static.emit('license')">
        <span tuiTitle>
          <span tuiSubtitle>{{ 'License' | i18n }}</span>
          {{ pkg().license }}
        </span>
        <tui-icon icon="@tui.chevron-right" />
      </button>
    </section>
    @if (trust) {
      <section tuiCardLarge appearance="secondary-grayscale">
        <header tuiHeader>{{ 'Signed by' | i18n }}</header>
        <marketplace-signers [keys]="signers()" />
      </section>
    }
    <section tuiCardLarge appearance="secondary-grayscale">
      <header tuiHeader>{{ 'Description' | i18n }}</header>
      <div [innerHTML]="pkg().description.long | localize"></div>
    </section>
  `,
  styles: `
    :host {
      display: contents;
    }
  `,
  imports: [
    MarketplaceVersionsComponent,
    MarketplaceSignersComponent,
    DatePipe,
    i18nPipe,
    TuiCell,
    TuiTitle,
    TuiIcon,
    TuiCardLarge,
    TuiHeader,
    LocalizePipe,
  ],
})
export class MarketplaceAboutComponent {
  protected readonly trust = inject(MarketplaceTrust, { optional: true })
  readonly copyService = inject(CopyService)

  readonly pkg = input.required<MarketplacePkgBase>()

  protected readonly signers = computed(() => packageSigners(this.pkg()))
  readonly versions = input<string[] | null>(null)

  readonly static = output<'license'>()
  readonly onVersion = output<string>()
}
