# Full-disk encryption contract

FDE is opt-in and targets UEFI systems with TPM 2.0 and Secure Boot. The immutable OS image and the encrypted data drive remain separate. The admin password independently unlocks the data volumes; TPM availability is not a prerequisite for recovering data with that password.

## Bootstrap identity

Boot attestation gates the private TLS identity used by the password-entry page and the network credentials needed to reach it. It does not release a data-volume password or data-decryption key. A failed attestation cannot silently enroll changed boot state or fall back to presenting the valid unlock identity.

The measured execution chain must authenticate the kernel, initramfs, effective command line, rootfs and persistent overlay before bootstrap credentials are released. The authorized policy must bind the verified loader path: an alternate trusted kernel running an unsigned initramfs must not gain access by replaying public UKI or filesystem measurement events. Filesystem contents must remain integrity-protected after verification. Merely authenticating a filename or measuring a mutable directory does not satisfy this contract.

`start_core::fde::bootstrap` authenticates and encrypts the bootstrap payload. `start_core::fde::tpm` seals its random encryption key to a P-256-authorized policy over SHA-256 PCRs 0, 2, 4, 7, 11, 14 and 15, restricted to `Unseal`. Firmware and option ROM measurements are pinned alongside Secure Boot and shim MOK policy. PCR 4 binds the loaded application sequence; PCR 11 belongs to the UKI measurement chain; PCR 15 binds the verified rootfs and overlay. Unexpected firmware or trust-policy changes require trusted re-enrollment, not automatic resealing. The policy-signing private key belongs on the encrypted data drive. The TPM policy authorization and the overlay manifest signature are distinct messages.

`start_core::fde::credentials` carries encrypted leaf TLS identities and NetworkManager profiles without a data-unlock password. Leaf identities share the certificate-chain type and TLS configuration builder used by the normal server. NetworkManager keyfile discovery is shared with the existing saved-profile owner. Bootstrap keyfiles are materialized with private permissions beneath `/run`, then removed individually during handoff; saved data-drive profiles remain authoritative.

These primitives alone do not establish the measured boot chain or enable FDE on a server.

## UKI construction

`build/lib/scripts/fde-build-uki` embeds the kernel, initramfs, effective command line and OS release in a native-architecture UKI, signed using the enrolled RSA Secure Boot identity. That private key belongs on the encrypted data drive and is distinct from the P-256 policy authority. PCR11 prediction uses the final image's actual virtual section contents through systemd tooling; Authenticode signing timestamps do not select boot state.

The authorized application sequence is firmware → shim → UKI, without GRUB or another command interpreter. Shim directly loads the machine-signed UKI from its second-stage EFI path. The supported measurement profile includes the whole UKI, not a separately measured embedded kernel. Extra applications, returned-image fallback paths and unsupported firmware measurement behavior fail closed; they are not learned as replacement policy. Vendor and Microsoft firmware trust may remain enrolled. The MOK certificate must permit image signing; a module-only certificate is insufficient.

PCR4/PCR7 prediction must validate the current event-log replay against live PCRs and preserve the trusted firmware/shim prefix. The future UKI and its actual enrolled authority determine the replacement events. Enrollment starts in a trusted installation or recovery environment; public-log consistency alone does not establish that trust. PCR14 binds MOK policy, including unused allowlist and denylist entries.

The helper creates a UKI and a `.pcr11` companion in a private staging generation on a hard-link-capable filesystem. The companion contains 64 lowercase hexadecimal digits and a newline, describing SHA-256 PCR11 before any initrd phase events. The pair is not independently atomic: the enclosing complete generation owns publication. Existing output files are not replaced on errors.

Activation makes the compatible rootfs, overlay and policy generation durable before replacing the complete EFI image on the ESP. Old dependencies remain until activation is durable. The old UKI may select only overlays with a compatible rootfs and an authorization for that boot/filesystem combination. FAT replacement does not guarantee boot availability across a power cut; signature and TPM failures remain fail-closed. Stub sidecars consumed before secret release must be authenticated or excluded.

## Persistent overlay

The persistent overlay is an independently signed squashfs. Boot uses immutable verified rootfs and overlay images with a temporary writable root layer. `/media/startos/config/overlay` exposes a separate editable view of the overlay squashfs with its own RAM-backed upper layer; editing that view must not mutate an active lower layer of the running root.

OS operations commit after editing; administrators may make manual edits and explicitly commit. Writers must quiesce while commit captures the working view. A commit publishes a complete, durable signed generation before attempting to remove older generations. Cleanup failure does not undo successful publication. Unpublished staging generations are not boot candidates.

`start_core::os_overlay` prepares, signs and durably publishes complete generations using the native SquashFS writer. Working-view export shares the filesystem merger used for OCI layers; RAM-upper deletion removes a configuration override, while untouched raw-lower whiteouts and opaque markers retain their rootfs deletion semantics. Working mounts disable `index`, `metacopy` and `redirect_dir`.

Boot chooses the newest complete, signature-valid, content-verified generation by timestamp. This is not anti-rollback protection: an attacker can delete newer generations. No signed generation counter or TPM-backed freshness floor is required.

## Existing installations

Migration replaces the legacy default-password keyslots with the admin password and replaces header backups under `/media/startos/config/luks`. It does not rotate volume keys or re-encrypt already encrypted volumes. Interrupted migration must remain recoverable across both data volumes; enrollment cannot complete while the default credential remains accepted.

`start_core::disk::fde::LuksPool` installs the new password before returning nonsecret retirement intent bound to volume identities and old keyslot IDs. The account owner durably commits this intent with the new password hash before retirement. Retirement resumes using the new password alone; the owner prevents keyslot reuse until it clears completed intent. Header replacement shares the normal disk-import owner. Unencrypted `_UNENC` pools require a separate encryption migration before this API applies.

Retiring obsolete header copies relies on Btrfs discard and storage returning zeroes for deallocated blocks, specifically NVMe DLFEAT=001b. This assumption is not a cryptographic secure-erasure guarantee and cannot revoke header copies previously taken off the device. Header backups copied into backupfs retain that filesystem's password-encrypted protection.
