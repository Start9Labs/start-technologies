import { MultiHost } from './Host'
import { RangeInterfaceBuilder } from './RangeInterfaceBuilder'

/**
 * Handle returned by {@link MultiHost.bindPortRange}. Mirrors {@link Origin}
 * but exports a single, restricted `api` service interface spanning the whole
 * range — a range is a homogeneous pool of ports (RTP media, ZMQ sockets, FTP
 * data), so it exposes exactly one interface. Distinct endpoints should be
 * separate {@link MultiHost.bindPortRange} calls.
 */
export class RangeOrigin {
  private exported = false

  constructor(
    readonly host: MultiHost,
    readonly internalStartPort: number,
    readonly externalStartPort: number,
    readonly numberOfPorts: number,
  ) {}

  /**
   * Register the range's single `api` service interface with StartOS.
   *
   * @param serviceInterface - a builder from `sdk.createRangeInterface`
   */
  async export(serviceInterface: RangeInterfaceBuilder): Promise<void> {
    if (this.exported) {
      throw new Error(
        `range at internal port ${this.internalStartPort} already exported an interface; a range exposes exactly one interface`,
      )
    }
    this.exported = true
    const { effects, id, name, description, scheme, index } =
      serviceInterface.options
    await effects.exportRangeServiceInterface({
      hostId: this.host.options.id,
      internalStartPort: this.internalStartPort,
      id,
      name,
      description,
      scheme: scheme ?? null,
      ...(index != null && { index }),
    })
  }
}
