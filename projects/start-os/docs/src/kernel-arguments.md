# Kernel Arguments

Persistent GRUB configuration lets administrators customize kernel arguments across reboots and StartOS updates. Configure it over [SSH](ssh.md) when troubleshooting with support or accommodating particular hardware.

> [!WARNING]
> Incorrect arguments can prevent booting or disable hardware. Keep required boot arguments, including `boot=startos`, and retain platform-specific settings unless deliberately changing them. Recovery may require local bootloader access or installer media.

## Persistent configuration

Store root-owned shell configuration files ending in `.cfg` under `/media/startos/config/grub.d/`. StartOS sources them in filename order after its image and platform defaults whenever GRUB configuration is generated, including during installation and OS updates. Files do not need executable permission; they execute as root when sourced.

For example, to append a kernel logging argument:

```bash
sudo install -d -m 0755 /media/startos/config/grub.d
sudo tee /media/startos/config/grub.d/50-user.cfg >/dev/null <<'EOF'
GRUB_CMDLINE_LINUX="$GRUB_CMDLINE_LINUX loglevel=4"
EOF
sudo update-grub
```

Reboot to apply the generated configuration. Read `/proc/cmdline` after reboot to see the arguments actually used by the running kernel. Editing a file alone does not change the running kernel or regenerate the boot configuration.

Use `GRUB_CMDLINE_LINUX` for arguments applying to all Linux entries, including recovery entries. `GRUB_CMDLINE_LINUX_DEFAULT` supplies additional arguments for normal entries. See the [GRUB configuration reference](https://www.gnu.org/software/grub/manual/grub/html_node/Simple-configuration.html) for their semantics.

To remove an argument without replacing the remaining defaults, modify the existing value in the drop-in. For example, to remove the default x86 serial-console argument:

```sh
GRUB_CMDLINE_LINUX="$(printf '%s\n' "$GRUB_CMDLINE_LINUX" | sed 's/ console=ttyS0,115200n8//g')"
```

A complete assignment replaces the list instead:

```sh
GRUB_CMDLINE_LINUX="boot=startos console=tty0"
GRUB_CMDLINE_LINUX_DEFAULT=""
```

That example removes the serial console on generic x86 systems, but also replaces any other arguments in those variables. A full replacement does not inherit future changes to StartOS defaults and is not suitable for every platform. GRUB also generates arguments such as `root=` outside these variables.

To undo a customization, remove its `.cfg` file, run `sudo update-grub`, and reboot. Do not edit `/boot/grub/grub.cfg` directly. Edits to `/etc/default/grub` are ephemeral, and files in the persistent `/etc` overlay are not the supported update-persistent interface; move intentional customizations into `/media/startos/config/grub.d/` instead. These settings survive OS updates, not a disk wipe.

## NVMe isolation

The diagnostic helper owns `/media/startos/config/grub.d/90-nvme-isolation.cfg`. Do not use that filename for administrator configuration. It does not enable a hardware workaround by default.

Run individual tests as directed by support:

```bash
sudo /usr/lib/startos/scripts/nvme-isolation-test apst
sudo reboot
sudo /usr/lib/startos/scripts/nvme-isolation-test collect
```

Available modes are `apst` (disable NVMe autonomous power-state transitions), `hmb` (disable Host Memory Buffer allocation), `aspm` (leave PCIe ASPM configuration to firmware), and `portpm` (disable PCIe port power management). Multiple modes can be combined, for example `apst aspm portpm`. `collect /dev/nvme2` selects a different controller; the default is `/dev/nvme0`.

Changing modes replaces the diagnostic file rather than accumulating earlier tests. The helper checks the generated Linux boot entries for the selected arguments and refuses conflicting administrator configuration. Failed generation or a conflict preserves the previous diagnostic file and boot configuration. The helper does not reboot or run a storage workload automatically.

To remove the diagnostic settings:

```bash
sudo /usr/lib/startos/scripts/nvme-isolation-test baseline
sudo reboot
```

Baseline removes only the diagnostic file, not independently configured arguments. If those include the tested power-management parameters, move them aside explicitly before an isolation run. Baseline does not override firmware settings or the kernel's built-in hardware quirks.
