# Volumes

Volumes are folders managed independently of services. A volume can use your server's data drive or a CIFS/SMB network share, with separate access grants for each service.

## Create and manage volumes

Open **System → Volumes** and create a volume with a name and a source. For a network source, enter the host, share path, username and password. The share path begins with the share name, such as `Photos/albums`, rather than the remote computer's filesystem path.

A network volume can be saved while its source is offline. Check its availability from the volume's system view. A successful check is an observation, not a guarantee that a remote device will remain connected.

Names are editable. Renaming preserves the volume's identity and service grants. Source settings are fixed; create a separate volume to use another source.

## Grant service access

Open the service's **Volumes** page. Each system volume has three choices:

- **Off**: no access.
- **Read**: the service can read the volume.
- **Read/write**: the service can read and modify the volume.

The **Volumes** tab appears only for services that support volumes. Creating a volume grants access to no services automatically. Multiple services can use the same volume with different access modes.

Revoking access detaches that service's attachments without detaching other consumers. How a service responds to missing storage depends on its package. Prefer volumes for optional media, documents and other shared files rather than operation-critical databases.

## Remove a volume

Removing a volume definition revokes its grants. Local contents are retained unless their deletion is separately confirmed. Removed local volumes remain under **Retained local data** until their data is deleted. Removing a network volume never deletes remote contents.

Uninstalling or rolling back a consuming service leaves volume contents untouched.

## Backup and restore

Select volumes independently alongside services when [creating a backup](backup-create.md). Each selected volume copies its complete backing folder, including network sources. An unavailable source fails that item rather than producing an empty successful copy.

Use a backup destination outside every selected volume, including any server-side aliases of those folders.

A service backup excludes its attached volume contents. Back up the volume itself to protect those files.

[Restoring a volume](backup-restore.md) creates a new local volume with a new identity and no grants, even when the backup came from a network share. Existing local and remote data stays untouched. Grant the restored copy to its consumers and select it again in any service settings that stored the original volume's identity.
