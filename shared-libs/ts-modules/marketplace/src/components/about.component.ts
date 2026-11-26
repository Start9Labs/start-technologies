import { DatePipe } from '@angular/common'
import { Component, computed, inject, input, output } from '@angular/core'
import {
  CopyService,
  i18nPipe,
  LocalizePipe,
  MarkdownPipe,
  SafeLinksDirective,
} from '@start9labs/shared'
import { TuiCell, TuiIcon, TuiTitle } from '@taiga-ui/core'
import { NgDompurifyPipe } from '@taiga-ui/dompurify'
import { TuiSkeleton } from '@taiga-ui/kit'
import { TuiCardLarge, TuiHeader } from '@taiga-ui/layout'
import { MarketplaceListing, MarketplacePkgBase } from '../types'
import { MarketplaceTrust, packageSigners } from '../trust'
import { MarketplaceSignerPreview } from './signer-preview'
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
          @if (loading()) {
            <span [tuiSkeleton]="1"></span>
          } @else {
            {{ pkg().sdkVersion || ('Unknown' | i18n) }}
          }
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
      <button
        type="button"
        tuiCell
        [disabled]="loading()"
        (click)="static.emit('license')"
      >
        <span tuiTitle>
          <span tuiSubtitle>{{ 'License' | i18n }}</span>
          @if (loading()) {
            <span [tuiSkeleton]="1"></span>
          } @else {
            {{ pkg().license }}
          }
        </span>
        <tui-icon icon="@tui.chevron-right" />
      </button>
    </section>
    @if (trust) {
      <section tuiCardLarge appearance="secondary-grayscale">
        <header tuiHeader>{{ 'Signed by' | i18n }}</header>
        <marketplace-signer-preview [keys]="signers()" />
      </section>
    }
    <section tuiCardLarge appearance="secondary-grayscale">
      <header tuiHeader>{{ 'Description' | i18n }}</header>
      @if (loading()) {
        <p [tuiSkeleton]="40"></p>
      } @else {
        <div
          class="g-markdown"
          safeLinks
          [innerHTML]="
            pkg().description.long ?? pkg().description.short
              | localize: pkg().defaultLocale
              | markdown
              | dompurify
          "
        ></div>
      }
    </section>
  `,
  styles: `
    :host {
      display: contents;
    }
  `,
  imports: [
    MarketplaceVersionsComponent,
    MarketplaceSignerPreview,
    DatePipe,
    i18nPipe,
    TuiCell,
    TuiTitle,
    TuiIcon,
    TuiSkeleton,
    TuiCardLarge,
    TuiHeader,
    LocalizePipe,
    MarkdownPipe,
    NgDompurifyPipe,
    SafeLinksDirective,
  ],
})
export class MarketplaceAboutComponent {
  protected readonly trust = inject(MarketplaceTrust, { optional: true })
  readonly copyService = inject(CopyService)

  readonly pkg = input.required<
    Pick<
      MarketplaceListing,
      'version' | 's9pks' | 'gitHash' | 'defaultLocale' | 'description'
    > &
      Partial<Pick<MarketplacePkgBase, 'sdkVersion' | 'license' | 'signers'>>
  >()

  /** Shows skeletons for what a listing lacks. */
  readonly loading = input(false)

  protected readonly signers = computed(() => packageSigners(this.pkg()))
  readonly versions = input<string[] | null>(null)

  readonly static = output<'license'>()
  readonly onVersion = output<string>()
}
