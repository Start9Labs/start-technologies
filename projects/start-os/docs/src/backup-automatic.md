# Automatic Backups

Automatic backups protect System data and selected services on a schedule. They
keep their checkpoints separate from the latest manual backup.

> [!IMPORTANT]
> A backup location is still a single point of failure. Protect important data
> on more than one high-quality drive or network folder.

## Protection and Encryption

Each schedule has its own backup location, timing, service selection, and
version-history policy. System data and installed services are selected by
default when the first schedule is created. A schedule can also include future
services automatically.
Select services individually, or use **Toggle all services** to change every
selection at once.

StartOS stops each selected service while copying its data, then starts it again
if it was running before the backup. Other services remain available. A service’s
backup procedure fails after six hours. Staging and checkpoint copies can add
time to the overall backup.
If StartOS cannot stop a timed-out backup procedure, it keeps the backup location
connected and retries shutdown before cleaning up. A notification directs you to
OS logs and recommends restarting the server if shutdown continues to fail.
If its runtime cannot recover afterward, the service remains stopped with an
error and a notification. Restart the server, then start the affected service.

Automatic backups use the same master-password encryption as manual backups.
StartOS uses the password to initialize or unlock the backup location but does
not store it. Changing the server password does not re-encrypt existing backups.
Use the current master password to authorize changes. When an existing location
needs to be unlocked, StartOS also asks for its original backup password if that
differs from the current password.

## Schedules

During setup, **Continue** validates each step. Select a completed step to return
to its settings.

Schedules can run hourly, daily, weekly, or monthly at a chosen local time and
timezone. StartOS stores the selected timezone with the schedule so
daylight-saving changes are handled correctly. Pausing, resuming, or changing a
schedule's name, services, or version history preserves completed occurrences,
including a repeated autumn hour. A monthly schedule set for a date that does
not occur in a given month runs on that month's final day.

Multiple schedules can protect different services, use different locations, or
run at different times. Pausing a schedule keeps its settings and checkpoints
in its active history so resumed backups continue from the existing history.
The schedule list updates as schedules run, pause, or change from another
browser or the command line.
Pause all and Resume all update every selected schedule together. If a schedule
cannot be resumed, the schedules keep their previous enabled states.
When no schedule includes future services, StartOS recommends adding each newly
installed service to one or more schedules; the recommendation can be dismissed.

Only one backup or restore operation runs at a time. Scheduled backups wait for
an active operation to finish. A second manual backup, restore, or explicit
automatic run is rejected rather than queued silently. Creating or changing a
schedule also requires the backup system to be free because StartOS verifies
the location's encrypted metadata before saving the schedule. If StartOS
restarts during an operation, the interrupted activity is recorded as failed
and stale progress is cleared. The next run reclaims incomplete staging data
on its backup location before checking available space. Backup storage is compacted
before the location is disconnected to reclaim space from deleted data.

## Version History and Storage

By default, StartOS keeps only the latest automatic checkpoint for each item.
Version-history rules can additionally retain one checkpoint per hour, day,
week, or month for a chosen duration. A month in version history is a rolling
30-day interval. A schedule may run less often than its most frequent
version-history rule. The admin UI warns that some intervals will have no
checkpoint but keeps every available interval without blocking the schedule.

When a selected service has checkpoints or another schedule on the chosen
location, its saved policy remains unchanged. Adding a schedule does not remove
existing checkpoints or restrict the new timing. An empty history detached from
every schedule adopts the new schedule's policy and timezone.
Editing a schedule's name or timing preserves the location's saved history policy.
Version-history rules can be changed before the first successful checkpoint.
Custom rules created through the CLI appear with their saved interval and
coverage in the web UI. Remove a custom rule and add a replacement to change it
through the UI.

Schedules that share a physical drive or network folder must select the same
backup-location entry. If StartOS reports that another schedule already uses
the location, select that existing entry.

Each retained checkpoint is a full copy on the backup location, not an
incremental delta. A run also needs temporary staging space. Keeping more
versions therefore increases storage use, run time, and I/O, especially on
network folders and slower drives. Capacity estimates account for current data,
retained checkpoints, and staging space. Before stopping services, StartOS checks
available space using current service data and its package archive, with previous
physical measurements as a lower bound.
The preflight check reserves space for a full copy of every selected item plus
safety headroom. Free space must cover the complete run before services stop,
even when its version-history policy replaces earlier checkpoints.
Use **Refresh estimates** after changing a schedule’s services or version-history
rules. Estimates follow the rules that
will apply to each history. The setup summary compares additional storage needed
with free space, accounting for checkpoints already on the location.

Retention applies to a service's shared automatic history on a backup location.
If several schedules use that history, StartOS previews the checkpoints a policy
change would remove and the schedules it would affect before applying it. When
editing a schedule, the confirmation lists the affected services and each
checkpoint's date and size.
If the location contains checkpoints missing from that preview, review the
updated history before confirming the change again.

Changing a schedule's location does not copy its existing checkpoints. They
remain archived on the old location, and the next run begins a history on the
new one. A replacement drive or network folder needs a separate location entry
while the original entry has schedules or checkpoints.
For a replacement network folder, add a new location entry. If a replacement
drive reuses the original device name, find its partition under
`/dev/disk/by-id/` over SSH and reassign the schedule with the CLI:

```sh
start-cli backup job reassign-target <JOB_ID> disk-/dev/disk/by-id/<REPLACEMENT_PARTITION> <PASSWORD> --wait-for-schedule
```

Deleting a schedule can either leave its automatic checkpoints archived
or remove checkpoints no longer referenced by another schedule. Archive decisions
made while a location is disconnected are preserved when it reconnects. If
checkpoint deletion is interrupted, removed checkpoints disappear from recovery
choices and their remaining data is reclaimed before the next automatic run.
Manual checkpoints are never removed by schedule deletion. Deleting archived
checkpoints after reconnecting a location requires the current master password.
Deleting a schedule together with its checkpoints verifies the current password
and backup location before removing the schedule. An incorrect password or a
canceled original-password prompt leaves the schedule and checkpoints intact.

## History, Restore, and Failures

Backup history records manual backups, automatic runs, and restores, including
service-level failures. An unreadable service archive appears as a failed restore
in history while other selected services continue restoring.
Search by schedule name, service, backup location, or
status in your selected language. Schedule names appear as you entered them.
Server data appears as **System** alongside the individual service reports.
When the automatic-backup card needs attention, **See more** opens Backup history.
For a single schedule, the switch and menu on the right pause or resume backups
and provide schedule actions.
History retains the newest 1,000 completed entries in addition
to any backup or restore still in progress. Successful checkpoints remain
available when another service in the same run fails. During restore, StartOS
selects the newest available checkpoint for each service by default, but any
retained or archived manual or automatic checkpoint can be chosen instead. See
[Restoring Backups](./backup-restore.md).

StartOS sends a notification whenever an automatic run fails. It pauses affected
schedules after three consecutive failures to connect to a backup location. It
also refuses to write when credentials are no longer valid, the location's
identity has changed, or its metadata is missing or invalid. Repair the original
location, provide current credentials, or explicitly move the schedule to
another location before resuming it. Discovery also limits the number of recovery
entries; if a location exceeds the limit shown in the error, use a location with
fewer entries.
An unreadable-location notification points to the technical details in Backup
history. Check the drive or network folder and its backup metadata before retrying;
an identity-change notification means the location no longer matches the saved
backup history.

The command-line backup interface can list and manage schedules, inspect
activity and checkpoints, preview retention changes, repair targets, and start
runs. See the [start-cli backup reference](./cli-reference.md#backups).
