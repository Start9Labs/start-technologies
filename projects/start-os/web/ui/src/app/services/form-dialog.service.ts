import { inject, Injectable, Injector, Type } from '@angular/core'
import { DialogService, i18nKey, i18nPipe } from '@start9labs/shared'
import { TuiDialogOptions } from '@taiga-ui/core'
import { TuiConfirmData, TuiConfirmService } from '@taiga-ui/kit'
import { PolymorpheusComponent } from '@taiga-ui/polymorpheus'
import { finalize, firstValueFrom, Observer, share } from 'rxjs'

@Injectable({ providedIn: 'root' })
export class FormDialogService {
  private readonly dialog = inject(DialogService)
  private readonly i18n = inject(i18nPipe)
  // Dialog lifetime owns discard state across inner-form rebuilds.
  private readonly formService = new TuiConfirmService()
  private readonly injector = Injector.create({
    parent: inject(Injector),
    providers: [
      {
        provide: TuiConfirmService,
        useValue: this.formService,
      },
    ],
  })

  open<T, R = void>(
    component: Type<any>,
    options: Partial<TuiDialogOptions<T>> & {
      label?: i18nKey
    } = {},
  ) {
    const closable = this.withConfirm()

    const result$ = this.dialog
      .openComponent<R>(new PolymorpheusComponent(component, this.injector), {
        dismissible: closable,
        closable,
        ...options,
      })
      .pipe(
        finalize(() => this.formService.markAsPristine()),
        share(),
      )

    result$.subscribe()

    return result$
  }

  async close(observer: Pick<Observer<unknown>, 'complete'>) {
    if (await firstValueFrom(this.withConfirm(), { defaultValue: false }))
      observer.complete()
  }

  private withConfirm() {
    const prompt: Partial<TuiDialogOptions<TuiConfirmData>> = {
      label: this.i18n.transform('Unsaved changes'),
      data: {
        content: this.i18n.transform(
          'You have unsaved changes. Are you sure you want to leave?',
        ),
        yes: this.i18n.transform('Leave'),
        no: this.i18n.transform('Cancel'),
      },
    }
    return this.formService.withConfirm(prompt)
  }
}
