---
release-notes: true
---

- Manage local folders, folders on external drives and CIFS shares in System → Volumes, with separate No Access, Read and Read/write grants for each supported service. Format drives and refresh availability with source-error details from the same page. Deleting a volume permanently removes managed local data but preserves external-drive and network-share contents.
- Services can treat missing registered folders as unavailable without accepting invalid mount requests.
- Back up volumes independently of services and restore them as new local copies without overwriting existing data.
