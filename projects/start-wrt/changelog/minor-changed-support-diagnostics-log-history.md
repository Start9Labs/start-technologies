- **Support diagnostics include history from before the last reboot.** The
  download now carries the router's saved system log, which spans reboots and
  each boot from its first line and holds 32–64 MiB of history, along with
  the activity log and a short system summary. It downloads as a `.log.gz`
  file.
- **System logs are kept across later updates and included in backups.** Restoring a
  backup keeps the router's own log, preceded by any older history the
  backup carried.
- **`/var` persists across reboots.** The system log lives at
  `/var/log/messages` and the activity log under `/var/lib/startwrt/`, where
  a standard Linux system keeps them.
