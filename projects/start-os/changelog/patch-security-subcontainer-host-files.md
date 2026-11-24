---
release-notes: true
---

- Confine subcontainer overlay creation, mount cleanup, and hostname/hosts writes to the intended container filesystem when images or runtimes replace paths.
- Prevent image-supplied metadata and replacement mounts from redirecting subcontainer cleanup; namespace-init signals use pinned process handles.
- Reject symlink targets when the host connects or reconnects to a service's runtime socket.
