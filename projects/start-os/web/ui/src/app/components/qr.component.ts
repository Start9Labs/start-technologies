import {
  ChangeDetectionStrategy,
  Component,
  computed,
  input,
} from '@angular/core'
import { QrCodeComponent } from 'ng-qrcode'

const utf8 = new TextEncoder()

/**
 * ng-qrcode draws a fixed-pixel canvas at correction level `M`, which fails two
 * ways once a payload gets long. Past what `M` can hold the encoder throws "The
 * amount of data is too big to be stored in a QR Code" and nothing renders at
 * all. Below that the code still draws, but a fixed 350px canvas leaves a
 * version-29 code with modules under 3px — too fine for a camera to resolve.
 *
 * So the canvas is drawn oversized and CSS decides the physical size, keeping
 * whatever room the container offers, and dense payloads drop to level `L`.
 */
@Component({
  selector: 'app-qr',
  template: `
    <qr-code
      styleClass="g-qr"
      [value]="value()"
      [errorCorrectionLevel]="level()"
      [size]="1024"
    />
  `,
  imports: [QrCodeComponent],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class QRComponent {
  readonly value = input.required<string>()

  /**
   * Byte-mode capacity of a version-40 code, which anything with a lowercase
   * letter in it encodes as: 2331 bytes at `M`, 2953 at `L`.
   */
  readonly level = computed(() =>
    utf8.encode(this.value()).length > 2331 ? 'L' : 'M',
  )
}
