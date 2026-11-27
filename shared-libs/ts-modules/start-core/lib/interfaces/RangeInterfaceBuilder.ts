import { Effects } from '../Effects'

/**
 * A restricted interface builder for port-range bindings, produced by
 * `sdk.createRangeInterface` and handed to {@link RangeOrigin.export}.
 *
 * Unlike {@link ServiceInterfaceBuilder}, a range interface is always
 * `api`-typed and carries no `masked` / `username` / `path` / `query`. Its
 * address is the host plus the range's external port span — there is no single
 * clickable URL. `scheme` is an optional transport prefix (e.g. `tcp` for
 * bitcoin ZMQ endpoints); omit it for raw UDP/TCP ranges (coturn, RTP, FTP).
 */
export class RangeInterfaceBuilder {
  constructor(
    readonly options: {
      effects: Effects
      id: string
      name: string
      description: string
      scheme?: string | null
      index?: number
    },
  ) {}
}
