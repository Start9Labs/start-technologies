import { Component, signal } from '@angular/core'
import { FormsModule } from '@angular/forms'
import { i18nKey, i18nPipe } from '@start9labs/shared'
import { tuiIsString } from '@taiga-ui/cdk'
import { TuiButton, TuiButtonX, TuiLoader } from '@taiga-ui/core'
import {
  TuiAvatar,
  TuiFiles,
  tuiInputFilesOptionsProvider,
} from '@taiga-ui/kit'
import { TitleDirective } from 'src/app/services/title.service'

import { SideloadPackageComponent } from './package.component'
import { MarketplacePkgSideload, validateS9pk } from './sideload.utils'

@Component({
  template: `
    <ng-container *title>{{ 'Sideload' | i18n }}</ng-container>
    @if (file && package(); as pkg) {
      <sideload-package [pkg]="pkg" [file]="file!">
        <button tuiButtonX (click)="clear()">{{ 'Close' | i18n }}</button>
      </sideload-package>
    } @else {
      <label tuiInputFiles (click)="clear()">
        <input
          tuiInputFiles
          accept=".s9pk"
          [ngModel]="null"
          (ngModelChange)="onFile($event)"
        />
        <ng-template>
          @if (error(); as err) {
            <div>
              <span tuiAvatar="@tui.circle-x" appearance="secondary"></span>
              <p class="g-negative">{{ err | i18n }}</p>
              <button tuiButton>{{ 'Try again' | i18n }}</button>
            </div>
          } @else if (file) {
            <div>
              <tui-loader size="l" />
              <p>{{ 'Checking package signatures' | i18n }}</p>
            </div>
          } @else {
            <div>
              <span tuiAvatar="@tui.upload" appearance="secondary"></span>
              <p>{{ 'Upload .s9pk package file' | i18n }}</p>
              <button tuiButton>{{ 'Select' | i18n }}</button>
            </div>
          }
        </ng-template>
      </label>
    }
  `,
  host: { class: 'g-page', '[style.padding-top.rem]': '2' },
  styles: `
    label {
      height: 100%;
      max-width: 42rem;
      margin: 0 auto;
    }
  `,
  providers: [tuiInputFilesOptionsProvider({ maxFileSize: Infinity })],
  imports: [
    FormsModule,
    TuiFiles,
    TuiAvatar,
    TuiButton,
    TuiButtonX,
    TuiLoader,
    SideloadPackageComponent,
    TitleDirective,
    i18nPipe,
  ],
})
export default class SideloadComponent {
  protected file: File | null = null
  protected readonly package = signal<MarketplacePkgSideload | null>(null)
  protected readonly error = signal<i18nKey | null>(null)

  protected clear() {
    this.file = null
    this.package.set(null)
    this.error.set(null)
  }

  protected async onFile(file: File | null) {
    this.file = file

    const parsed = file ? await validateS9pk(file) : ''
    if (this.file !== file) return

    this.package.set(tuiIsString(parsed) ? null : parsed)
    this.error.set(tuiIsString(parsed) ? (parsed as i18nKey) : null)
  }
}
