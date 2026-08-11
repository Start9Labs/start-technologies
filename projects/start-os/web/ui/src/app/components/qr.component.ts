import {
  ChangeDetectionStrategy,
  Component,
  computed,
  input,
} from '@angular/core'
import { QrCodeComponent } from 'ng-qrcode'

const utf8 = new TextEncoder()

/**
 * ng-qrcode encodes at correction level `M`, which has no version left past
 * what that level can hold: the encoder throws "The amount of data is too big
 * to be stored in a QR Code" and the modal comes up empty, with only a console
 * error to say why. Level `L` carries the payload instead.
 */
@Component({
  selector: 'app-qr',
  template: `
    <qr-code [value]="value()" [errorCorrectionLevel]="level()" size="350" />
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
