# Volumes

Volumes are folders managed independently of services. A volume can use your server's data drive, an attached external-drive partition or a CIFS/SMB network share, with separate access grants for each service.

## Create and manage volumes

Open **System → Volumes** and create a volume with a name and a source:

- **Local folder** creates a managed folder on the server's data drive.
- **External drive** selects a connected partition from the same drive discovery used by Backups and a folder path relative to that partition. Missing folders are created; an empty path or `/` selects the filesystem root. The selected folder becomes the volume root, and services cannot use it to access parent or sibling folders. Its filesystem UUID identifies it independently of device names and USB ports. Keep it connected while services use it. The filesystem must support Linux ownership mapping; mounting does not rewrite its existing file ownership.
- **CIFS share** takes a host, share path, username and password. The share path begins with the share name, such as `Photos/albums`, rather than the remote computer's filesystem path. A network volume can be saved while its source is offline.

The table's **Refresh** button updates status and free space for every volume. For local folders it checks the directory and available data-drive space; it is not a connectivity test. CIFS rows use the same signal bars as Backups. A successful refresh is an observation, not a guarantee that a device will remain connected.

**Format a drive** opens the same formatting dialog as Backups. It permanently erases the selected drive and creates one Btrfs partition. System and active mounted drives are protected. Formatting a drive changes its filesystem UUID: an existing volume for its old filesystem becomes unavailable rather than acquiring access to the replacement.

Names are editable. Renaming preserves the volume's identity and service grants. Source settings are fixed; create a separate volume to use another source.

## Grant service access

Open the service's **Volumes** page. Each system volume has three choices:

- **No Access**: no access.
- **Read**: the service can read the volume.
- **Read/write**: the service can read and modify the volume.

The **Volumes** tab appears only for services that support volumes. Creating a volume grants access to no services automatically. Multiple services can use the same volume with different access modes.

Revoking access detaches that service's attachments without detaching other consumers. How a service responds to missing storage depends on its package. Prefer volumes for optional media, documents and other shared files rather than operation-critical databases.

## Remove a volume

Choose **Remove** from a volume's menu to revoke service access. The confirmation depends on the source:

- For a managed **Local folder**, the red **Delete** button permanently deletes the folder and its contents after attachments are detached.
- For an **External drive** or **CIFS share**, **Remove** deletes the volume configuration but leaves the source's contents untouched.

If detachment or local cleanup fails, the error is reported. Managed local contents remain under **Pending local deletion** for a retry; they are not deleted while detachment has failed.

Uninstalling or rolling back a consuming service leaves volume contents untouched.

## Backup and restore

Select volumes independently alongside services when [creating a backup](backup-create.md). Each selected volume copies its complete backing folder, including network sources. An unavailable source fails that item rather than producing an empty successful copy.

Use a backup destination outside every selected volume, including any server-side aliases of those folders. An external-drive volume cannot be backed up onto the same partition.

A service backup excludes its attached volume contents. Back up the volume itself to protect those files.

[Restoring a volume](backup-restore.md) creates a new local volume with a new identity and no grants, even when the backup came from an external drive or network share. Existing local and remote data stays untouched. Grant the restored copy to its consumers and select it again in any service settings that stored the original volume's identity.
