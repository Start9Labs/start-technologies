# Volumes

System volumes are user-owned local folders or CIFS/SMB shares granted to selected services. They are separate from package-owned `sdk.volumes` and dependency mounts.

## Declare support

Add this property to `setupManifest`:

```ts
sharedVolumes: true,
```

This enables the service's **Volumes** controls in StartOS; it requests no access. Users grant each volume independently as read-only or read/write. `buildManifest` requires StartOS 0.4.1 for this capability and rejects an explicit older compatibility override.

## Read grants

```ts
const grants = await sdk.getSharedVolumes(effects).const()
```

Each grant contains `id`, `name`, `readonly` and `availability`. Store the stable GUID `id` in service configuration and use the editable `name` in user-facing choices. Names are neither identities nor paths. Host paths and network credentials remain private to StartOS.

The reader supports `.once()`, `.const()`, `.watch()` and the other [reactive reader methods](file-models.md). Grant additions, revocations, access-mode changes and renames invalidate reactive reads. An offline granted volume remains listed; revocation removes it. Availability is the host's last observation, not a mount preflight or a guarantee of continued connectivity.

## Optional storage

Prefer volumes for optional media, documents and other shared files. Keep operation-critical state in package-owned volumes.

Mount an optional volume explicitly and inspect the outcome before launching its workload:

```ts
import { T } from '@start9labs/start-sdk'
import { sdk } from './sdk'

export async function indexOptionalVolume(effects: T.Effects, id: string) {
  const grant = (await sdk.getSharedVolumes(effects).once()).find(v => v.id === id)
  if (!grant) return

  const sub = await sdk.SubContainer.eager(effects, { imageId: 'main' }, null, 'volume-indexer')
  try {
    const outcome = await sub.tryMountSharedVolume({
      id: grant.id,
      mountpoint: '/media',
      subpath: 'albums',
      readonly: grant.readonly,
    })
    if (outcome.kind === 'unavailable') return outcome

    await sub.execFail(['index-media', '/media'])
    return outcome
  } finally {
    await sub.destroy()
  }
}
```

An unavailable source returns `{ kind: 'unavailable', reason: 'sourceUnavailable' | 'timeout' }`. Authorization, invalid paths and internal errors reject instead. Do not catch every rejection as an optional outage. A failed or revoked attachment leaves an inaccessible, read-only placeholder, not writable local storage.

Report an unavailable optional feature through its existing health check and keep unaffected daemons running. Preserve the selected ID. Retry the affected workload through the existing daemon retry or reconciliation lifecycle; a successful mount can later lose connectivity, and the application must handle I/O errors. The one-shot helper above does not retry itself.

## Essential workload mounts

`Mounts.mountSharedVolume` makes the attachment essential to that subcontainer:

```ts
export function essentialVolumeMount(grant: T.GrantedSharedVolume) {
  return sdk.Mounts.of().mountSharedVolume({
    id: grant.id,
    mountpoint: '/media',
    readonly: grant.readonly,
  })
}
```

Unavailability rejects construction with the volume ID, mountpoint and reason. A separate optional-feature daemon can use this declaration and the existing daemon startup retries without preventing unrelated daemons from starting. Make the entire service depend on it only when the workload is genuinely essential.

StartOS checks the manifest capability, current grant, access mode and confined paths when attaching. Read-only access cannot be escalated by requesting read/write. Subpaths must remain within the granted source. Revocation detaches the service's attachments without detaching other consumers; applications must respond through their reactive configuration and normal workload lifecycle.

## Backup ownership

System volumes are independent backup items. A service's backup must describe its package-owned state, including saved volume IDs where appropriate, rather than copy shared contents.

For services declaring this capability, StartOS runs the backup procedure in a fresh runtime with shared mounts disabled. Initialization and uninitialization hooks do not run in that backup runtime. Read backup configuration from persistent state rather than variables populated by initialization hooks. Service restore also rejects shared mounts until restoration has finished.

A restored volume is a new local copy with a new ID and no grants, including when its source was CIFS. Users grant and select the restored copy again. Uninstalling or rolling back a consuming service leaves shared contents untouched.
