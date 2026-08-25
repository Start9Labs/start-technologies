import {
  ChangeDetectionStrategy,
  Component,
  computed,
  input,
} from '@angular/core'
import { i18nPipe } from '@start9labs/shared'
import { QrCodeComponent, QrCodeErrorCorrectionLevel } from 'ng-qrcode'
import { PlaceholderComponent } from './placeholder.component'

const utf8 = new TextEncoder()

/** Version-40 capacity, the largest symbol there is, by encoding mode. */
const CAPACITY = {
  numeric: { M: 5596, L: 7089 },
  alphanumeric: { M: 3391, L: 4296 },
  byte: { M: 2331, L: 2953 },
} as const

function measure(value: string) {
  if (/^[0-9]*$/.test(value)) return { ...CAPACITY.numeric, size: value.length }
  if (/^[0-9A-Z $%*+\-./:]*$/.test(value))
    return { ...CAPACITY.alphanumeric, size: value.length }
  return { ...CAPACITY.byte, size: utf8.encode(value).length }
}

@Component({
  selector: 'app-qr',
  template: `
    @if (level(); as level) {
      <qr-code [value]="value()" [errorCorrectionLevel]="level" size="350" />
    } @else {
      <app-placeholder icon="@tui.qr-code">
        {{ 'Too long for a QR code' | i18n }}
      </app-placeholder>
    }
  `,
  imports: [QrCodeComponent, PlaceholderComponent, i18nPipe],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class QRComponent {
  readonly value = input.required<string>()

  /** `null` when no version-40 symbol holds the payload — the encoder throws. */
  readonly level = computed<QrCodeErrorCorrectionLevel | null>(() => {
    const { M, L, size } = measure(this.value())
    return size <= M ? 'M' : size <= L ? 'L' : null
  })
}
