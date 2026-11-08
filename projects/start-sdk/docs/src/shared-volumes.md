# Shared Volumes

Shared volumes are for data that several services, or other devices, need to use; a service's own data belongs in its package-owned volumes. Declare support only if your service benefits from data the user shares with it.

Shared volumes are user-owned local folders, folders on attached drives or CIFS/SMB shares granted to selected services. They are separate from package-owned `sdk.volumes` and dependency mounts.

## Declare support

Use Start SDK 4.1.0 or later. Add this property to `setupManifest`:

```ts
sharedVolumes: true,
```

This enables the service's **Shared Volumes** controls in StartOS; it requests no access. Users grant each volume independently as read-only or read/write. `buildManifest` requires StartOS 0.4.2 for this capability and rejects an explicit older compatibility override. Packaging it requires Start CLI 3.0.0 or later; the build wrapper checks before packing and verifies the resulting archive preserves the opt-in. Unreleased builds can report 3.0.0 before including this capability. If archive verification fails, install a volume-capable CLI build; the failed output is removed.

## Read grants

```ts
const grants = await sdk.getSharedVolumes(effects).const()
```

Each grant contains `id`, `name`, `readonly` and `availability`. Store the stable GUID `id` in service configuration and use the editable `name` in user-facing choices. Names are neither identities nor paths. The API returns neither host paths nor network credentials. Attached-drive mounts translate ownership through StartOS's existing service UID/GID mapping without changing on-disk owners. Linux mount metadata can reveal the CIFS account name to a granted service; its password remains private.

The reader supports `.once()`, `.const()`, `.watch()` and the other [reactive reader methods](file-models.md). Reactive reads are invalidated only when one of the service's own granted volumes changes in a way its mounts depend on: a grant is added or revoked, its access mode changes, it becomes available or unavailable (a status reset to unknown does not count), or its source changes, including updated CIFS connection details. Renames and changes to other volumes do not invalidate them, so a returned name can be stale until the next read. Updating a CIFS share's connection details does not change an existing mount, which keeps the source it opened until the service mounts the volume again. An offline granted volume remains listed; revocation removes it. Availability is the host's last observation, not a mount preflight or a guarantee of continued connectivity.

Inside `Daemons.dynamic`, a `.const()` catalog invalidation refreshes containers with declared or attempted shared-volume mounts, including failed optional attempts, even if returned metadata is unchanged. Their shared-container peers and dependents restart too; unrelated containers keep running. The callback covers the service's granted catalog rather than an individual source, so shared-volume consumers refresh together. Create fresh lazy subcontainers inside each builder evaluation. Nested effects retain the reconciler's retry scope rather than restarting the entire service.

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

An unavailable source returns `{ kind: 'unavailable', reason: 'sourceUnavailable' | 'timeout' }`. Authorization, invalid paths and internal errors reject instead. Do not catch every rejection as an optional outage. An unavailable source or successful revocation leaves its tracked mountpoint inaccessible and read-only.

Mount outside package-owned and dependency volumes, on the subcontainer's own filesystem. A directory created inside a persistent package volume survives later starts even when nothing mounts there; writes can then land in that directory instead of the shared source. Use the stable volume ID rather than its editable name when deriving paths. If a persistent mountpoint is necessary, remove stale empty directories at startup before launching the workload.

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

Grants do not pass through dependency mounts. A service reading another service's package volume sees the underlying directory, not shared-volume attachments nested inside it. Each consuming service needs its own grant and mount.

StartOS checks the manifest capability, current grant, access mode and confined paths when attaching. Read-only access cannot be escalated by requesting read/write. Subpaths must remain within the granted source. Revocation detaches the service's attachments without detaching other consumers; applications must respond through their reactive configuration and normal workload lifecycle.

## Backup ownership

Shared volumes are independent backup items. A service's backup must describe its package-owned state, including saved volume IDs where appropriate, rather than copy shared contents.

For services declaring this capability, StartOS runs the backup procedure in a fresh runtime with shared mounts disabled. Initialization and uninitialization hooks do not run in that backup runtime. Read backup configuration from persistent state rather than variables populated by initialization hooks. Service restore also rejects shared mounts until restoration has finished.

A restored volume is a new local copy with a new ID and no grants, including when its source was an attached drive or CIFS. Users grant and select the restored copy again. Uninstalling or rolling back a consuming service leaves shared contents untouched.
