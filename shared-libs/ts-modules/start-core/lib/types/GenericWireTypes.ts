import type { RecoverySourceWire, RegistryAssetWire } from '../osBindings'

type ReplaceField<Owner, Field extends PropertyKey, Value> = {
  [Key in keyof Owner]: Key extends Field ? Value : Owner[Key]
}

/** A migration or backup recovery source with a caller-selected backup password representation. */
export type RecoverySource<Password> = ReplaceField<
  RecoverySourceWire,
  'password',
  Password
>

/** A registry asset with a caller-selected commitment representation. */
export type RegistryAsset<Commitment> = ReplaceField<
  RegistryAssetWire,
  'commitment',
  Commitment
>
