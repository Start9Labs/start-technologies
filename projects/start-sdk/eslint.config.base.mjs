// Shared StartOS package lint config. Packages import this from their own
// eslint.config.mjs. Type-aware: catches the SDK 2.0 footgun where a lazy
// SubContainer accessor (rootfs/guid/subpath()) — a Promise — is used in a
// string context without await, which tsc cannot catch in a template literal.
import tseslint from 'typescript-eslint'

export default tseslint.config({
  files: ['startos/**/*.ts'],
  languageOptions: {
    parser: tseslint.parser,
    parserOptions: { projectService: true },
  },
  plugins: { '@typescript-eslint': tseslint.plugin },
  rules: {
    // `${sub.rootfs}` where rootfs is Promise<string> -> "[object Promise]"
    '@typescript-eslint/restrict-template-expressions': [
      'error',
      {
        allowNumber: true,
        allowBoolean: true,
        allowNullish: true,
        allowRegExp: true,
      },
    ],
  },
})
