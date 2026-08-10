import {
  ChangeDetectionStrategy,
  Component,
  computed,
  input,
} from '@angular/core'
import { QrCodeComponent } from 'ng-qrcode'

/**
 * ng-qrcode draws a fixed-pixel canvas at correction level `M`, which fails two
 * ways once a payload gets long. Past 3391 alphanumeric characters level `M`
 * has no version left, the encoder throws "The amount of data is too big to be
 * stored in a QR Code" and nothing renders at all. Below that the code still
 * draws, but a fixed 350px canvas leaves a version-29 code with modules under
 * 3px — too fine for a hardware wallet's camera to resolve.
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

  /** Alphanumeric capacity of a version-40 code: 3391 at `M`, 4296 at `L`. */
  readonly level = computed(() => (this.value().length > 3391 ? 'L' : 'M'))
}
