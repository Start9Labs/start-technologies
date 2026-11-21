---
release-notes: false
---

- Read-only service and SubContainer mounts protect nested filesystems. Attachment and container teardown preserve source filesystems and other live mounts.
- Idmapped file binds and file/directory target changes use the requested target type.
- UID/GID-remapped mount trees retain existing nested filesystems rather than following later source mounts.
