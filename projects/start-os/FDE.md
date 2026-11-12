# Full-disk encryption contract

FDE is opt-in and targets UEFI systems with TPM 2.0 and Secure Boot. The immutable OS image and the encrypted data drive remain separate. The admin password independently unlocks the data volumes; TPM availability is not a prerequisite for recovering data with that password.

## Bootstrap identity

Boot attestation gates the private TLS identity used by the password-entry page and the network credentials needed to reach it. It does not release a data-volume password or data-decryption key. A failed attestation cannot silently enroll changed boot state or fall back to presenting the valid unlock identity.

The measured execution chain must authenticate the kernel, initramfs, effective command line, rootfs and persistent overlay before bootstrap credentials are released. The authorized policy must bind the verified loader path: an alternate trusted kernel running an unsigned initramfs must not gain access by replaying public UKI or filesystem measurement events. Filesystem contents must remain integrity-protected after verification. Merely authenticating a filename or measuring a mutable directory does not satisfy this contract.

`start_core::fde::bootstrap` authenticates and encrypts the bootstrap payload. `start_core::fde::tpm` seals its random encryption key to a P-256-authorized policy over SHA-256 PCRs 0, 2, 4, 7, 11, 12, 13, 14 and 15, restricted to `Unseal`. The policy reference binds the canonical bank/register selection; an authorization for a weaker selection cannot unlock the object. Firmware and option ROM measurements are pinned alongside Secure Boot and shim MOK policy. PCR 4 binds the loaded application sequence; PCR 11 belongs to the UKI measurement chain; PCR 15 binds the verified rootfs and overlay. Unexpected firmware or trust-policy changes require trusted re-enrollment, not automatic resealing. The policy-signing private key belongs on the encrypted data drive. The TPM policy authorization and the overlay manifest signature are distinct messages.

`fde::tpm::MeasurementProfile` pairs public expected PCR values with their authorization. Reuse as authorization inputs requires signature verification and recomputation of the TPM policy digest from those values. A valid signature on a policy digest alone does not authenticate adjacent measurement metadata. Observed post-phase PCR values are not future pre-phase predictions.

`start_core::fde::credentials` carries encrypted TLS material and NetworkManager profiles without a data-unlock password. Local certificate issuance shares the normal server's intermediate-only issuer and certificate cache; the bootstrap issuer excludes the root CA private key. ACME snapshots contain the selected providers' settings, accounts and cached identities, and reuse normal issuance and challenge handling. Leaf identities share the normal certificate-chain type and TLS configuration builder. NetworkManager keyfile discovery is shared with the existing saved-profile owner. Bootstrap keyfiles are materialized with private permissions beneath `/run`, then removed individually during handoff; saved data-drive profiles remain authoritative.

`net::net_controller::plan_host_exposure` owns exposure decisions for normal and standalone consumers. Standalone interface discovery and vhost listeners accept saved input, interface watches and injected TLS sources without constructing private-database state.

These primitives alone do not establish the measured boot chain or enable FDE on a server.

## UKI construction

`build/lib/scripts/fde-build-uki` embeds the kernel, initramfs, effective command line and OS release in a native-architecture UKI, signed using the enrolled RSA Secure Boot identity. That private key belongs on the encrypted data drive and is distinct from the P-256 policy authority. PCR11 prediction uses the final image's actual virtual section contents through systemd tooling; Authenticode signing timestamps do not select boot state.

The authorized application sequence is firmware → shim → UKI, without GRUB or another command interpreter. Shim directly loads the machine-signed UKI from its second-stage EFI path. The supported measurement profile includes the whole UKI, not a separately measured embedded kernel. Extra applications, returned-image fallback paths and unsupported firmware measurement behavior fail closed; they are not learned as replacement policy. Vendor and Microsoft firmware trust may remain enrolled. The MOK certificate must permit image signing; a module-only certificate is insufficient.

PCR4/PCR7 prediction must validate the current event-log replay against live PCRs and preserve the trusted firmware/shim prefix. The future UKI and its actual enrolled authority determine the replacement events. Enrollment starts in a trusted installation or recovery environment; public-log consistency alone does not establish that trust. PCR14 binds MOK policy, including unused allowlist and denylist entries.

`start_core::fde::loader` captures a caller-authenticated current chain, including legacy GRUB and kernel artifacts, and retains only the firmware/shim prefix. Future predictions pair the exact UKI application digest with its enrolled MOK authority event. The validated legacy tail is not a future authorization. Baselines belong in the encrypted database, never public boot configuration.

The helper creates a UKI and a `.pcr11` companion in a private staging generation on a hard-link-capable filesystem. The companion contains 64 lowercase hexadecimal digits and a newline, describing SHA-256 PCR11 before any initrd phase events. The pair is not independently atomic: the enclosing complete generation owns publication. Existing output files are not replaced on errors.

Activation makes the compatible rootfs, overlay and policy generation durable before replacing the complete EFI image on the ESP. Old dependencies remain until activation is durable. The old UKI may select only overlays with a compatible rootfs and an authorization for that boot/filesystem combination. FAT replacement does not guarantee boot availability across a power cut; signature and TPM failures remain fail-closed. The supported profile excludes external stub inputs by requiring empty PCRs 12 and 13 when authorizing. This excludes external parameters, credentials, configuration/system extensions, nondefault UKI profiles and addons; public boot metadata cannot authorize them.

## Boot selection

`fde::boot::Descriptor` pins one rootfs filename and the manifest/policy verification keys inside the signed initramfs. Unsigned configuration cannot replace those keys or choose a non-FDE boot mode. Complete generations live under `config/fde/generations`; their public policy files remain outside the hashed configuration image to avoid a filesystem-policy hash cycle.

Selection reuses `os_overlay::verified_generations` ordering and admits the newest candidate whose authenticated policy matches the observed boot plus its anticipated filesystem event. A newer signed filesystem without a matching boot authorization does not displace an older compatible generation. `Descriptor::mount` opens and fully reads both signed verity mappings and mounts them as read-only SquashFS. `MountedGeneration::measure` checks the live boot again before extending PCR15. Mount guards remove mappings after successful unmount, or transfer both resources to the boot owner for handoff. Selection alone cannot release credentials; dependent mounts must end before their mappings close.

`startbox start-fde-prepare` takes the OS-volume mount and target root mountpoint. It reads the descriptor from the authenticated FDE initramfs, not disk configuration; guarded release composes the running and working roots before measuring the protected filesystems and decrypting the bootstrap snapshot. Its caller must terminate on error without ordinary-boot fallback or an interactive shell. The normal initramfs does not invoke it.

The preparation helper excludes active swap and piped core handlers, disables its own dumpability and inherits a zero core limit into TPM tools. These conditions must remain stable until release finishes. Runtime identities and TPM workspaces use private `noswap` tmpfs mounts; the private handoff remains RAM-only after helper exit. This initramfs protection does not establish the separate OS-boot-lifetime volume-key cache or protect later daemon handling by itself.

`fde-harden-initramfs.mjs` transforms the reviewed initramfs-tools 0.148.4 control plane in a caller-owned private tree. It rejects changed or unknown providers/configuration before mutation. Failure handling never returns to boot, including when reboot fails; break requests and alternate-init fallback cannot open a shell. The transform fixes the FDE boot route and removes resume, live boot, keymap execution, runtime parameter sourcing and automatic `/usr` mounts. Ordinary generated images are not transformed.

This control-plane transform is not a complete image builder or executable-closure audit. Udev rules/helpers, copied tool dependencies and final archive verification remain prerequisites before signing or activating an FDE image.

## Persistent overlay

The persistent overlay is an independently signed squashfs. Boot uses immutable verified rootfs and overlay images with a temporary writable root layer. `/media/startos/config/overlay` exposes a separate editable view of the overlay squashfs with its own RAM-backed upper layer; editing that view must not mutate an active lower layer of the running root.

The running root layers the immutable configuration over the immutable public image, with a separate RAM upper for runtime writes. The configuration-only working mount lives inside the running root's config bind. `BootHandoff::working_upper` records its raw upper path for snapshot preparation. All composed mounts end before verified lower mappings close.

OS operations commit after editing; administrators may make manual edits and explicitly commit. Writers must quiesce while commit captures the working view. A commit publishes a complete, durable signed generation before attempting to remove older generations. Cleanup failure does not undo successful publication. Unpublished staging generations are not boot candidates.

`start_core::os_overlay` prepares, signs and durably publishes complete generations using the native SquashFS writer. Working-view export shares the filesystem merger used for OCI layers; RAM-upper deletion removes a configuration override, while untouched raw-lower whiteouts and opaque markers retain their rootfs deletion semantics. Working mounts disable `index`, `metacopy` and `redirect_dir`. `VerityImage::open` reads every block through the new mapping before returning it, closes failed initial mappings, and rejects later corruption with I/O errors. The boot owner retains mappings until their dependent mounts are gone.

Boot chooses the newest complete, signature-valid, content-verified generation by timestamp. This is not anti-rollback protection: an attacker can delete newer generations. No signed generation counter or TPM-backed freshness floor is required.

## Existing installations

Migration replaces the legacy default-password keyslots with the admin password and replaces header backups under `/media/startos/config/luks`. It does not rotate volume keys or re-encrypt already encrypted volumes. Interrupted migration must remain recoverable across both data volumes; enrollment cannot complete while the default credential remains accepted.

`start_core::disk::fde::LuksPool::plan_password_change` reads identity-bound intent without mutating either LV. The account owner persists this plan and the target password hash before installation, retaining exclusive keyslot ownership until completion or abort. Installation verifies both target slots before the owner commits the new account hash and administrator-slot authority. Retirement follows that commit and resumes using the new password alone. Abort precedes account commit and preserves the complete base inventory, including unrelated manual keys. A different target requires successful abort and a fresh plan. Header replacement shares the normal disk-import owner. Unencrypted `_UNENC` pools require a separate encryption migration before this API applies.

An authorized password reset requires only the new password, not the old plaintext. Password unlock may capture volume keys for the caller's protected volatile OS-boot-lifetime cache, surviving daemon restarts but not reboot. These keys never enter persistent storage, backups, PatchDB, bootstrap state, logs or an additional management keyslot. The boot owner protects their handling from swap, core dumps and diagnostics.

Retiring obsolete header copies relies on Btrfs discard and storage returning zeroes for deallocated blocks, specifically NVMe DLFEAT=001b. This assumption is not a cryptographic secure-erasure guarantee and cannot revoke header copies previously taken off the device. Header backups copied into backupfs retain that filesystem's password-encrypted protection.
