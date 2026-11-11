# Full-disk encryption contract

FDE is opt-in and targets UEFI systems with TPM 2.0 and Secure Boot. The immutable OS image and the encrypted data drive remain separate. The admin password independently unlocks the data volumes; TPM availability is not a prerequisite for recovering data with that password.

## Bootstrap identity

Boot attestation gates the private TLS identity used by the password-entry page and the network credentials needed to reach it. It does not release a data-volume password or data-decryption key. A failed attestation cannot silently enroll changed boot state or fall back to presenting the valid unlock identity.

The measured execution chain must authenticate the kernel, initramfs, effective command line, rootfs and persistent overlay before bootstrap credentials are released. Filesystem contents must remain integrity-protected after verification. Merely authenticating a filename or measuring a mutable directory does not satisfy this contract.

`start_core::fde::bootstrap` authenticates and encrypts the bootstrap payload. `start_core::fde::tpm` seals its random encryption key to a P-256-authorized policy over SHA-256 PCRs 7, 11 and 15, restricted to `Unseal`. PCR 11 belongs to the UKI measurement chain; PCR 15 binds the verified rootfs and overlay. The policy-signing private key belongs on the encrypted data drive. The TPM policy authorization and the overlay manifest signature are distinct messages.

`start_core::fde::credentials` carries encrypted leaf TLS identities and NetworkManager profiles without a data-unlock password. Leaf identities share the certificate-chain type and TLS configuration builder used by the normal server. NetworkManager keyfile discovery is shared with the existing saved-profile owner. Bootstrap keyfiles are materialized with private permissions beneath `/run`, then removed individually during handoff; saved data-drive profiles remain authoritative.

These primitives alone do not establish the measured boot chain or enable FDE on a server.

## Persistent overlay

The persistent overlay is an independently signed squashfs. Boot uses immutable verified rootfs and overlay images with a temporary writable root layer. `/media/startos/config/overlay` exposes a separate editable view of the overlay squashfs with its own RAM-backed upper layer; editing that view must not mutate an active lower layer of the running root.

OS operations commit after editing; administrators may make manual edits and explicitly commit. Writers must quiesce while commit captures the working view. A commit publishes a complete, durable signed generation before attempting to remove older generations. Cleanup failure does not undo successful publication. Unpublished staging generations are not boot candidates.

Boot chooses the newest complete, signature-valid, content-verified generation by timestamp. This is not anti-rollback protection: an attacker can delete newer generations. No signed generation counter or TPM-backed freshness floor is required.

## Existing installations

Migration replaces the legacy default-password keyslots with the admin password and replaces header backups under `/media/startos/config/luks`. It does not rotate volume keys or re-encrypt already encrypted volumes. Interrupted migration must remain recoverable across both data volumes; enrollment cannot complete while the default credential remains accepted.

Retiring obsolete header copies relies on Btrfs discard and storage returning zeroes for deallocated blocks, specifically NVMe DLFEAT=001b. This assumption is not a cryptographic secure-erasure guarantee and cannot revoke header copies previously taken off the device. Header backups copied into backupfs retain that filesystem's password-encrypted protection.
