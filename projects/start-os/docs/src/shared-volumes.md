# Shared Volumes

Shared volumes are only for data that several services, or other devices, need to use, such as a media library several services read or files a NAS shares with other computers. Each service already stores its own data, so most services never need a shared volume; use one only when data has to be shared.

A shared volume is a folder managed independently of services. It can use your server's data drive, an attached external-drive partition or a CIFS/SMB network share, with separate access grants for each service.

## Create and manage volumes

Open **System → Shared Volumes**, choose **Add**, pick a source and give the volume a name. A CIFS share is named by its host:

- **Local folder** creates a managed folder on the server's data drive.
- **External drive** first selects a connected drive, then one of its partitions or **Erase and format this drive**, suggests a name from the partition's label or the drive's make and model, and optionally a folder on it, such as `media/photos`. The server's own system and data drives are not offered. Missing folders are created; leaving the path blank uses the whole partition. The selected folder becomes the volume root, and services cannot use it to access parent or sibling folders. Its filesystem UUID identifies it independently of device names and USB ports. Keep it connected while services use it. The filesystem must support Linux ownership mapping; mounting does not rewrite its existing file ownership.
- **CIFS share** takes a host, share path, username and password. The share path begins with the share name, such as `Photos/albums`, rather than the remote computer's filesystem path; a leading `/` is optional. For SMB compatibility requirements and share setup on another computer or NAS, see [CIFS Share](backups.md#cifs-share). A network volume can be saved while its source is offline. Saving records the definition without connecting; the next status check tests the source and credentials and reports mount failures.

The volume list shows each volume's status and free space. When you open the page, StartOS rechecks any volume that has not been checked in the last 10 minutes. For local folders the check covers the directory and available data-drive space; it is not a connectivity test. A successful check is an observation, not a guarantee that a device will remain connected.

**Erase and format this drive** permanently erases the drive after a confirmation and creates one Btrfs partition labeled with the volume name, then creates the volume on it. If the volume cannot be created, the drive stays formatted and its new partition can be selected. System and active mounted drives are protected. Formatting a drive changes its filesystem UUID: an existing volume for its old filesystem becomes unavailable rather than acquiring access to the replacement.

Select a volume's settings button to open **Manage**, where you can see its details, rename it (a CIFS share is renamed by changing its host), change service access and remove it; changes apply when you **Save**. For a CIFS share you can also update the host, path, username and password; leave the password blank to keep it. Edits keep the volume's identity and service grants. Services that read their volumes reactively restart to use the new connection details. Other services keep their existing mount, including its old source and credentials, until they remount or restart. The catalog and new independent backups use the edited source immediately; editing is not access revocation. The source of a local folder or external drive is fixed; create a separate volume to use another one.

## Grant service access

Use a volume's **Manage** dialog, or the service's **Shared Volumes** page. Each service has three choices for each volume:

- **No Access**: no access.
- **Read**: the service can read the volume.
- **Read/write**: the service can read and modify the volume.

The **Shared Volumes** tab appears only for services that support shared volumes. Creating a volume grants access to no services automatically. Multiple services can use the same volume with different access modes.

Revoking access detaches that service's attachments without detaching other consumers. How a service responds to missing storage depends on its package. Prefer volumes for optional media, documents and other shared files rather than operation-critical databases.

## Remove a volume

Remove a volume from its **Manage** dialog. Removing revokes all service access, and what happens to the data depends on the source:

- For a managed **Local folder**, the button reads **Delete**. It permanently deletes the folder and its contents after attachments are detached, so StartOS asks you to type the volume's name to confirm.
- For an **External drive** or **CIFS share**, **Remove** deletes the volume configuration but leaves the source's contents untouched.

If detachment or local cleanup fails, the error is reported. Managed local contents remain under **Pending local deletion** for a retry; they are not deleted while detachment has failed.

Uninstalling or rolling back a consuming service leaves volume contents untouched.

## Backup and restore

Select volumes independently alongside services when [creating a backup](backups.md#creating-a-backup). Each selected volume copies its complete backing folder, including network sources. An unavailable source fails that item rather than producing an empty successful copy.

Use a backup destination outside every selected volume, including any server-side aliases of those folders. Disjoint folders on the same partition can be used. Configured paths are compared without case distinctions, and opened directory identities are checked for overlapping contents.

A service backup excludes its attached volume contents. Back up the volume itself to protect those files.

[Restoring a volume](backups.md#restoring-individual-services-or-volumes) creates a new local volume with a new identity and no grants, even when the backup came from an external drive or network share. Existing local and remote data stays untouched. Grant the restored copy to its consumers and select it again in any service settings that stored the original volume's identity.
