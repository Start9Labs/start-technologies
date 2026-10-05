import { Component } from '@angular/core'
import { TuiDialogContext } from '@taiga-ui/core'
import { injectContext } from '@taiga-ui/polymorpheus'
import { QRComponent } from 'src/app/routes/portal/components/qr.component'

@Component({
  selector: 'qr',
  template: '<app-qr [value]="context.data" />',
  imports: [QRComponent],
})
export class QRModal {
  readonly context = injectContext<TuiDialogContext<void, string>>()
}
