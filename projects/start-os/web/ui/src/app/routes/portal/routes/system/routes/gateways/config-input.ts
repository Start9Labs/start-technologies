import { i18nPipe } from '@start9labs/shared'
import { ISB } from '@start9labs/start-core'
import { TUI_INPUT_FILES_DEFAULT_OPTIONS } from '@taiga-ui/kit'

export function wireguardConfigInput(i18n: i18nPipe) {
  return ISB.Value.union({
    name: i18n.transform('WireGuard Config File'),
    default: 'paste',
    variants: ISB.Variants.of({
      paste: {
        name: i18n.transform('Copy/Paste'),
        spec: ISB.InputSpec.of({
          file: ISB.Value.textarea({
            name: i18n.transform('File Contents'),
            default: null,
            required: true,
            minRows: 8,
            maxRows: 8,
          }),
        }),
      },
      select: {
        name: i18n.transform('Upload'),
        spec: ISB.InputSpec.of({
          file: ISB.Value.browserFile({
            name: i18n.transform('File'),
            required: true,
            extensions: ['.conf'],
            maxSize: TUI_INPUT_FILES_DEFAULT_OPTIONS.maxFileSize,
          }),
        }),
      },
    }),
  })
}

export function readWireguardConfig(
  input: ReturnType<typeof wireguardConfigInput>['_TYPE'],
): Promise<string> {
  return input.selection === 'paste'
    ? Promise.resolve(input.value.file)
    : input.value.file.text()
}
