---
release-notes: true
---

- Share data between services, or with other devices, using local folders, folders on external drives and CIFS shares in System → Shared Volumes, with separate No Access, Read and Read/write grants for each supported service. Format a drive while adding it as a volume, update a CIFS share's connection details, and see each volume's availability, free space and source errors on the same page. Deleting a volume permanently removes managed local data but preserves external-drive and network-share contents.
- Attached-drive lists update automatically. Shared-volume status, free space and source errors refresh after drive and network changes, with mounted shares following the kernel's connection state and periodic free-space checks.
- Services can treat missing registered folders as unavailable without accepting invalid mount requests.
- Back up volumes independently of services and restore them as new local copies without overwriting existing data.
