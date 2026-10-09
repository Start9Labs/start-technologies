import { AsyncPipe } from '@angular/common'
import { Component, inject } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { FormsModule } from '@angular/forms'
import {
  TuiButton,
  TuiCheckbox,
  TuiDialogContext,
  TuiDialogOptions,
  TuiGroup,
  TuiLoader,
} from '@taiga-ui/core'
import { TuiBlock } from '@taiga-ui/kit'
import { injectContext, PolymorpheusComponent } from '@taiga-ui/polymorpheus'
import { PatchDB } from 'patch-db-client'
import { map, take } from 'rxjs'
import { PkgIconPipe } from 'src/app/routes/portal/pipes/pkg-icon'
import { DataModel } from 'src/app/services/patch-db/data-model'
import { getManifest } from 'src/app/utils/get-package-data'

interface Package {
  id: string
  title: string
  disabled: boolean
  checked: boolean
}

@Component({
  template: `
    <div tuiGroup orientation="vertical" [collapsed]="true">
      @if (pkgs(); as pkgs) {
        @for (pkg of pkgs; track $index) {
          <label tuiBlock>
            <img class="icon" alt="" [src]="pkg.id | pkgIcon | async" />
            {{ pkg.title }}
            <input
              type="checkbox"
              tuiCheckbox
              [disabled]="pkg.disabled"
              [(ngModel)]="pkg.checked"
              (ngModelChange)="handleChange()"
            />
          </label>
        } @empty {
          No services installed!
        }
      } @else {
        <tui-loader />
      }
    </div>
    <footer class="g-buttons">
      <button tuiButton appearance="flat-grayscale" (click)="toggleSelectAll()">
        Toggle all
      </button>
      <button tuiButton [disabled]="!hasSelection" (click)="done()">
        {{ context.data?.btnText || 'Done' }}
      </button>
    </footer>
  `,
  styles: `
    :host {
      display: flex;
      flex-direction: column;
      margin-top: 1.5rem;
    }

    .icon {
      width: 2.5rem;
      border-radius: 100%;
    }
  `,
  imports: [
    FormsModule,
    TuiButton,
    TuiGroup,
    TuiLoader,
    TuiBlock,
    TuiCheckbox,
    AsyncPipe,
    PkgIconPipe,
  ],
})
export class BackupsBackupModal {
  private readonly patch = inject<PatchDB<DataModel>>(PatchDB)
  readonly context =
    injectContext<TuiDialogContext<string[], { btnText: string } | undefined>>()

  hasSelection = false

  readonly pkgs = toSignal(
    this.patch.watch$('packageData').pipe(
      take(1),
      map(pkgs =>
        Object.values(pkgs)
          .map(pkg => {
            const { id, title } = getManifest(pkg)
            return {
              id,
              title,
              disabled: pkg.stateInfo.state !== 'installed',
              checked: false,
            }
          })
          .sort((a, b) =>
            b.title.toLowerCase() > a.title.toLowerCase() ? -1 : 1,
          ),
      ),
    ),
    { initialValue: null },
  )

  done() {
    this.context.completeWith(
      this.pkgs()
        ?.filter(p => p.checked)
        .map(p => p.id) || [],
    )
  }

  handleChange() {
    this.hasSelection = !!this.pkgs()?.some(p => p.checked)
  }

  toggleSelectAll() {
    this.pkgs()?.forEach(p => (p.checked = !this.hasSelection && !p.disabled))
    this.hasSelection = !this.hasSelection
  }
}

export const BACKUP = new PolymorpheusComponent(BackupsBackupModal)

export const BACKUP_OPTIONS: Partial<TuiDialogOptions<unknown>> = {
  label: 'Select services',
}
