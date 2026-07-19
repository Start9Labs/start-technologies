import { Component, inject, input } from '@angular/core'
import {
  MarketplaceAboutComponent,
  MarketplaceDependenciesComponent,
  MarketplaceLinksComponent,
  MarketplaceReleaseNotesComponent,
} from '@start9labs/marketplace'
import { DialogService, EmptyPipe, MARKDOWN } from '@start9labs/shared'
import { TuiTitle } from '@taiga-ui/core'
import { TuiAvatar, TuiFade } from '@taiga-ui/kit'
import { TuiHeader } from '@taiga-ui/layout'
import { of } from 'rxjs'
import { MarketplaceControlsComponent } from '../marketplace/components/controls.component'
import { MarketplacePkgSideload } from './sideload.utils'

@Component({
  selector: 'sideload-package',
  template: `
    <header tuiHeader="h4">
      <span tuiAvatar>
        <img
          alt=""
          [src]="pkg().icon || 'assets/img/service-icons/fallback.png'"
        />
      </span>
      <span tuiTitle tuiFade>{{ pkg().title }}</span>
      <span tuiAccessories>
        <marketplace-controls [pkg]="pkg()" [file]="file()" />
        <ng-content />
      </span>
    </header>
    <marketplace-about [pkg]="pkg()" (static)="onStatic()" />
    <marketplace-release-notes [pkg]="pkg()" />
    @if (!(pkg().dependencyMetadata | empty)) {
      <marketplace-dependencies [pkg]="pkg()" />
    }
    <marketplace-links [pkg]="pkg()" />
  `,
  styles: `
    :host {
      display: grid;
      gap: 1rem;
      inline-size: min(100%, 42rem);
      margin: auto;
    }

    header {
      gap: 1rem;
      align-items: center;
      white-space: nowrap;
      overflow: hidden;
    }
  `,
  imports: [
    EmptyPipe,
    MarketplaceAboutComponent,
    MarketplaceLinksComponent,
    MarketplaceDependenciesComponent,
    MarketplaceControlsComponent,
    MarketplaceReleaseNotesComponent,
    TuiHeader,
    TuiAvatar,
    TuiTitle,
    TuiFade,
  ],
})
export class SideloadPackageComponent {
  private readonly dialog = inject(DialogService)

  readonly pkg = input.required<MarketplacePkgSideload>()
  readonly file = input.required<File>()

  onStatic() {
    this.dialog
      .openComponent(MARKDOWN, {
        label: 'License',
        size: 'l',
        data: of(this.pkg()['license']),
      })
      .subscribe()
  }
}
