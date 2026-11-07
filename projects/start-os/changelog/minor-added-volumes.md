---
release-notes: true
---

- Share data between services, or with other devices, using local folders, folders on external drives and CIFS shares in System → Shared Volumes, with separate No Access, Read and Read/write grants for each supported service. Format a drive while adding it as a volume, update a CIFS share's connection details, and see each volume's availability, free space and source errors on the same page. Deleting a volume permanently removes managed local data but preserves external-drive and network-share contents.
- Keep unsaved names while volume or backup-target status updates arrive. CIFS connection changes notify reactive services; existing mounts keep their source until remounted or restarted.
- Services can treat missing registered folders as unavailable without accepting invalid mount requests.
- Back up volumes independently of services and restore them as new local copies without overwriting existing data.
