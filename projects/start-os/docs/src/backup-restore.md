# Restoring Backups

Restore a manual or automatic checkpoint to recover individual services, or use
a server backup during disaster recovery after a data-drive failure.

## Restoring Individual Services

An individual restore can recover service data from a manual or automatic
checkpoint. A service that is already installed must be uninstalled before its
checkpoint can be restored. Restore requires access to the physical drive or
network folder containing the backup and the master password used to encrypt it.

StartOS lists services with saved manual or automatic checkpoints on the selected
location. It chooses the newest checkpoint for each service by default, but a
different retained or archived checkpoint can be selected for any service.
Backups from another server remain available for restore alongside this server's
own scheduled history. Unlock them with the source server's backup password.
If one checkpoint source cannot be opened, the picker shows a warning alongside
the readable checkpoints. Check the location or retry with that source's original
password. Manual and automatic backups encrypted with different passwords can
be restored in separate passes.

Only one backup or restore can run at a time. A second request is rejected, while
scheduled backups wait for the active operation to finish. If StartOS restarts
during a restore, the interrupted operation is recorded as failed and stale
progress is cleared.

Backup history records each selected service's result. If a service archive is
unreadable, its restore fails while the other selected services continue.

For command-line recovery, use `start-cli backup history list` or `backup
history discover` to find automatic checkpoint IDs, then `start-cli package
backup restore-checkpoint` to select one checkpoint per service. See the
[start-cli reference](./cli-reference.md#activity-and-checkpoint-history).

> [!TIP]
> If the backup came from a different system architecture (x86, ARM, or RISC-V),
> StartOS runs its service images under emulation. After the restore, _reinstall_
> or update each service from the marketplace so StartOS can select its package
> for the new server architecture. Do not uninstall it, since uninstalling deletes
> its data.

## Restoring an Entire Server

If the StartOS data drive is lost or corrupted, follow the [recovery options
during initial setup](./initial-setup.md#recover-options). StartOS can recover
from a manual server backup or the newest automatic System checkpoint on the
selected location. Automatic recovery also restores the newest available
checkpoint for each service on that location.
