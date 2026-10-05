import { GetSystemSmtp, Patterns } from '../../util'
import { InputSpec } from './builder/inputSpec'
import { Value } from './builder/value'
import { Variants } from './builder/variants'
import { z } from '../../zExport'

const securityVariants = Variants.of({
  tls: {
    name: 'TLS',
    spec: InputSpec.of({
      port: Value.dynamicText(async () => ({
        name: 'Port',
        required: true,
        default: '465',
        disabled: 'Fixed for TLS',
      })),
    }),
  },
  starttls: {
    name: 'STARTTLS',
    spec: InputSpec.of({
      port: Value.select({
        name: 'Port',
        default: '587',
        values: { '25': '25', '587': '587', '2525': '2525' },
      }),
    }),
  },
})

/**
 * Creates an SMTP field spec with provider-specific defaults pre-filled.
 */
function smtpFields(
  defaults: {
    host?: string
    security?: 'starttls' | 'tls'
    hostDisabled?: boolean
  } = {},
) {
  const hostSpec = Value.text({
    name: 'Host',
    required: true,
    default: defaults.host ?? null,
    placeholder: 'smtp.example.com',
  })

  return InputSpec.of({
    host: defaults.hostDisabled
      ? hostSpec.withDisabled('Fixed for this provider')
      : hostSpec,
    security: Value.union({
      name: 'Connection Security',
      default: defaults.security ?? 'tls',
      variants: securityVariants,
    }),
    from: Value.text({
      name: 'From Address',
      required: true,
      default: null,
      placeholder: 'Example Name <test@example.com>',
      patterns: [Patterns.emailWithName],
    }),
    username: Value.text({
      name: 'Username',
      required: true,
      default: null,
    }),
    password: Value.text({
      name: 'Password',
      required: false,
      default: null,
      masked: true,
    }),
  })
}

/**
 * Base SMTP settings with no provider-specific defaults.
 */
export const customSmtp = smtpFields()

/**
 * Provider presets for SMTP configuration.
 * Each variant has SMTP fields pre-filled with the provider's recommended settings.
 */
export const smtpProviderVariants = Variants.of({
  gmail: {
    name: 'Gmail',
    spec: smtpFields({
      host: 'smtp.gmail.com',
      security: 'tls',
      hostDisabled: true,
    }),
  },
  ses: {
    name: 'Amazon SES',
    spec: smtpFields({
      host: 'email-smtp.us-east-1.amazonaws.com',
      security: 'tls',
    }),
  },
  sendgrid: {
    name: 'SendGrid',
    spec: smtpFields({
      host: 'smtp.sendgrid.net',
      security: 'tls',
      hostDisabled: true,
    }),
  },
  mailgun: {
    name: 'Mailgun',
    spec: smtpFields({
      host: 'smtp.mailgun.org',
      security: 'tls',
      hostDisabled: true,
    }),
  },
  protonmail: {
    name: 'Proton Mail',
    spec: smtpFields({
      host: 'smtp.protonmail.ch',
      security: 'starttls',
      hostDisabled: true,
    }),
  },
  other: {
    name: 'Other',
    spec: customSmtp,
  },
})

/**
 * System SMTP settings with provider presets.
 * Wraps smtpProviderVariants in a union for use by the system email settings page.
 */
export const systemSmtpSpec = InputSpec.of({
  provider: Value.union({
    name: 'Provider',
    default: 'gmail',
    variants: smtpProviderVariants,
  }),
})

const smtpVariants = Variants.of({
  disabled: { name: 'Disabled', spec: InputSpec.of({}) },
  system: {
    name: 'System Credentials',
    spec: InputSpec.of({
      customFrom: Value.text({
        name: 'Custom From Address',
        description:
          'A custom from address for this service. If not provided, the system from address will be used.',
        required: false,
        default: null,
        placeholder: 'Name <test@example.com>',
        patterns: [Patterns.emailWithName],
      }),
    }),
  },
  custom: {
    name: 'Custom Credentials',
    spec: InputSpec.of({
      provider: Value.union({
        name: 'Provider',
        default: null as any,
        variants: smtpProviderVariants,
      }),
    }),
  },
})
/**
 * For service inputSpec. Gives users 3 options for SMTP: (1) disabled, (2) use system SMTP settings, (3) use custom SMTP settings with provider presets
 */
export const smtpInputSpec = Value.dynamicUnion(async ({ effects }) => {
  const smtp = await new GetSystemSmtp(effects).once()
  const disabled = smtp ? [] : ['system']
  return {
    name: 'SMTP',
    description: 'Optionally provide an SMTP server for sending emails',
    default: 'disabled',
    disabled,
    variants: smtpVariants,
  }
}, smtpVariants.validator)

const securityShape = z
  .looseObject({
    selection: z.enum(['tls', 'starttls']).catch('tls'),
    value: z
      .looseObject({ port: z.string().catch('465') })
      .catch({ port: '465' }),
  })
  .catch({ selection: 'tls' as const, value: { port: '465' } })

const providerShape = z
  .looseObject({
    selection: z.string().catch('other'),
    value: z
      .looseObject({
        host: z.string().catch(''),
        from: z.string().catch(''),
        username: z.string().catch(''),
        password: z.string().nullable().optional().catch(null),
        security: securityShape,
      })
      .catch({
        host: '',
        from: '',
        username: '',
        password: null,
        security: securityShape.parse(undefined),
      }),
  })
  .catch({
    selection: 'other',
    value: {
      host: '',
      from: '',
      username: '',
      password: null,
      security: securityShape.parse(undefined),
    },
  })

export type SmtpSelection =
  | { selection: 'disabled'; value: Record<string, never> }
  | { selection: 'system'; value: { customFrom?: string | null } }
  | {
      selection: 'custom'
      value: {
        provider: {
          selection: string
          value: {
            host: string
            from: string
            username: string
            password?: string | null
            security: {
              selection: 'tls' | 'starttls'
              value: { port: string }
            }
          }
        }
      }
    }

/**
 * Zod schema for persisting SMTP selection in a store file model.
 * Use this instead of `smtpInputSpec.validator` to avoid cross-zod-instance issues.
 */
export const smtpShape: z.ZodType<SmtpSelection> = z
  .discriminatedUnion('selection', [
    z.looseObject({
      selection: z.literal('disabled'),
      value: z.looseObject({}).catch({}),
    }),
    z.looseObject({
      selection: z.literal('system'),
      value: z
        .looseObject({
          customFrom: z.string().nullable().optional().catch(null),
        })
        .catch({ customFrom: null }),
    }),
    z.looseObject({
      selection: z.literal('custom'),
      value: z
        .looseObject({ provider: providerShape })
        .catch({ provider: providerShape.parse(undefined) }),
    }),
  ])
  .catch({ selection: 'disabled' as const, value: {} }) as any

/**
 * Convert a stored SmtpSelection to a value suitable for prefilling smtpInputSpec.
 *
 * The stored type (SmtpSelection from smtpShape) uses flat unions for provider/security
 * selection, while the input spec (smtpInputSpec) uses distributed discriminated unions
 * (UnionRes). These are structurally incompatible in TypeScript's type system, even
 * though the runtime values are identical. This function bridges the two types so that
 * service code doesn't need `as any`.
 */
export function smtpPrefill(smtp: SmtpSelection | null | undefined): any {
  return smtp || undefined
}
