import {
  Component,
  computed,
  Directive,
  inject,
  Injector,
  OnDestroy,
} from '@angular/core'
import { FormsModule } from '@angular/forms'
import { TuiAnimated } from '@taiga-ui/cdk'
import { TuiPopup, TuiPopupService, TuiScrollbar } from '@taiga-ui/core'
import { NgDompurifyPipe } from '@taiga-ui/dompurify'
import { TuiBlock, TuiSwitch } from '@taiga-ui/kit'
import {
  injectContext,
  PolymorpheusComponent,
  provideContext,
} from '@taiga-ui/polymorpheus'
import { HELP_OPEN, HELP_URL, HelpService } from 'src/app/help/help'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'
import { MarkdownPipe } from '@start9labs/shared'

@Component({
  template: `
    <label tuiBlock="s" appearance="secondary-grayscale">
      <input type="checkbox" tuiSwitch size="s" [(ngModel)]="open" />
      {{ 'Help' | i18n }}
    </label>
    <tui-scrollbar *tuiPopup="open()" tuiAnimated class="modal-help">
      <div class="g-help" [innerHTML]="content() | markdown | dompurify"></div>
    </tui-scrollbar>
  `,
  styles: `
    :host {
      position: fixed;
      inset-block-start: 0.5rem;
      inset-inline-end: 3rem;
      backdrop-filter: blur(1rem);
    }

    :host,
    [tuiBlock] {
      border-radius: 10rem;
    }

    .modal-help {
      position: fixed;
      inset: 3rem 1rem 1rem auto;
      width: 18rem;
      border-radius: var(--tui-radius-l);
      background: var(--tui-background-elevation-1);
      box-shadow: var(--tui-shadow-popup);
      outline: 1px solid var(--tui-border-normal);
      outline-offset: -1px;

      --tui-from: translate3d(150%, 0, 0);

      &.tui-enter,
      &.tui-leave {
        animation-name: tuiSlide;
      }
    }

    ::ng-deep tui-root._mobile .modal-help {
      width: calc(100% - 2rem);
    }
  `,
  imports: [
    FormsModule,
    TuiBlock,
    TuiSwitch,
    TuiPopup,
    TuiAnimated,
    TuiScrollbar,
    i18nPipe,
    MarkdownPipe,
    NgDompurifyPipe,
  ],
})
class ModalToggle {
  protected readonly open = inject(HELP_OPEN)
  private readonly help = inject(HelpService)
  private readonly url = injectContext<string>()
  // Resolve reactively so the panel re-translates if the language switches
  // while it is open (and once a lazily-loaded language finishes loading).
  protected readonly content = computed(() => this.help.content()[this.url])
}

@Directive({
  selector: 'ng-template[modalHelp]',
})
export class ModalHelp implements OnDestroy {
  readonly ref = inject(TuiPopupService).add(
    new PolymorpheusComponent(
      ModalToggle,
      Injector.create({
        providers: [provideContext(inject(HELP_URL))],
      }),
    ),
  )

  ngOnDestroy() {
    this.ref.destroy()
  }
}
