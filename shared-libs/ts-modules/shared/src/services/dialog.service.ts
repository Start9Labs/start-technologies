import { inject, Service } from '@angular/core'
import {
  TuiResponsiveDialogOptions,
  TuiResponsiveDialogService,
} from '@taiga-ui/addon-mobile'
import { TUI_CONFIRM, TuiConfirmData } from '@taiga-ui/kit'
import {
  PolymorpheusComponent,
  PolymorpheusContent,
} from '@taiga-ui/polymorpheus'
import { PROMPT, PromptOptions } from '../components/prompt.component'
import { i18nPipe } from '../i18n/i18n.pipe'
import { i18nKey } from '../i18n/i18n.providers'
import { escapeHtml } from '../util/escape-html'

@Service()
export class DialogService {
  private readonly dialogs = inject(TuiResponsiveDialogService)
  private readonly i18n = inject(i18nPipe)

  openPrompt<T = void>(
    options: Partial<TuiResponsiveDialogOptions<PromptOptions>> & {
      label: i18nKey
      data: PromptOptions
    },
  ) {
    const { message, label, placeholder, buttonText } = options.data

    return this.dialogs.open<T>(PROMPT, {
      label: this.label(options.label),
      data: {
        ...options.data,
        message: this.i18n.transform(message),
        label: this.i18n.transform(label),
        placeholder: this.i18n.transform(placeholder),
        buttonText: this.i18n.transform(buttonText),
      },
    })
  }

  openConfirm(
    options: Partial<TuiResponsiveDialogOptions<TuiConfirmData>> & {
      label: i18nKey
      data?: TuiConfirmData & {
        content?: PolymorpheusComponent<any> | i18nKey
        yes?: i18nKey
        no?: i18nKey
      }
    },
  ) {
    const { content, yes, no } = options.data || {}

    return this.dialogs.open<boolean>(TUI_CONFIRM, {
      ...options,
      label: this.label(options.label),
      data: {
        ...(options.data || {}),
        content: isI18n(content) ? this.i18n.transform(content) : content,
        yes: this.i18n.transform(yes),
        no: this.i18n.transform(no),
      },
    })
  }

  openAlert<T = void>(
    message: i18nKey | undefined,
    options: Partial<TuiResponsiveDialogOptions<any>> & {
      label?: i18nKey
    } = {},
  ) {
    return this.dialogs.open<T>(this.i18n.transform(message), {
      ...options,
      label: this.label(options.label),
    })
  }

  openComponent<T = void>(
    component: PolymorpheusContent,
    options: Partial<TuiResponsiveDialogOptions<any>> & {
      label?: i18nKey
    } = {},
  ) {
    return this.dialogs.open<T>(component, {
      ...options,
      label: this.label(options.label),
    })
  }
  private label(key: i18nKey | undefined) {
    return escapeHtml(this.i18n.transform(key) || '')
  }
}

function isI18n(
  content: PolymorpheusComponent<any> | i18nKey | undefined,
): content is i18nKey {
  return typeof content === 'string'
}
