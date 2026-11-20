---
release-notes: false
---

- Installing StartOS with a GPT partition table onto a drive with 4096-byte sectors no longer fails. A GPT install onto a drive with a logical sector size other than 512 or 4096 bytes is refused before its partition table is rewritten.
