# Installing StartOS

This guide is for flashing StartOS to a USB drive, then installing it onto a desktop, laptop, or mini PC. For an up-to-date list of known-good hardware, please check out this [forum post](https://community.start9.com/t/known-good-hardware-master-list-hardware-capable-of-running-startos/). For Raspberry Pi, which does not use the USB installer, see [Raspberry Pi](#raspberry-pi) below.

## Watch The Video

<div class="yt-video" data-id="5hGeaBzMfWQ" data-title="Installing StartOS"></div>

## Download

1.  Visit the [Github release page](https://github.com/Start9Labs/start-technologies/releases/tag/start-os/v0.4.0.2) to find the latest version of StartOS.

1.  Under "Image Downloads", select the image for your hardware. StartOS is available in x86_64 (AMD64), aarch64 (ARM64), and RISC-V (RVA23). For x86_64 and aarch64, two variants are available:
    - **Standard**: Includes proprietary firmware and drivers for broader hardware compatibility, including display and wireless. Recommended for most users.

    - **Slim (FOSS-only)**: 100% open source, containing **no** proprietary firmware or drivers. Only compatible with certain hardware, such as the Start9 Server Pure.

    An **NVIDIA** variant is also published for x86_64 and aarch64. Choose it only if your server has an NVIDIA GPU you want services to use for computation — it adds NVIDIA's driver and container toolkit on top of the Standard image. It supports GeForce RTX 20-series, Quadro RTX and newer; older cards such as the GeForce GTX 900- and 10-series, or Tesla M40, P40, P100 and V100, are not supported and should use the Standard image.

## Verify your download

Before you flash it, check that the file on your computer is exactly the file Start9 published — that nothing was damaged on the way down, and nothing was swapped for it. It takes about a minute, and it is worth doing before you erase a drive.

You do not need to understand what any of this means. Follow the three steps and read the one word at the end.

### 1. Find your file on the release page

On the [release page](https://github.com/Start9Labs/start-technologies/releases/tag/start-os/v0.4.0.2), scroll down to **OS Images Checksums**, then to the block under **SHA-256**. It holds one line per image: a long code, then the filename it belongs to.

```text
37b63c86197150866809d34b5824ae22c5fc705d4f8dc9e9750b8fa23485441a  startos-0.4.0.1-fdb27c7_x86_64-nonfree.iso
```

Rather than reading down the list, press **Ctrl + F** (**Command + F** on a Mac), paste the name of the file you downloaded, and press Enter. Your browser jumps straight to its line.

### 2. Check your file against that line

{{#tabs global="platform"}}
{{#tab name="Mac"}}

Select and copy that **whole line** — the long code, the spaces, and the filename together.

Open Terminal: press Command + Space, type `Terminal`, and press Enter.

Type this and press Enter. It points Terminal at your downloads folder:

```sh
cd ~/Downloads
```

Now type this, replacing the words `PASTE THE WHOLE LINE HERE` with what you copied — keep the quotation marks — and press Enter:

```sh
echo "PASTE THE WHOLE LINE HERE" | shasum -a 256 -c
```

{{#endtab}}
{{#tab name="Windows"}}

From that line you need its two halves separately: the long code, and the filename.

Open PowerShell: press the Windows key, type `PowerShell`, and press Enter.

Type this, replacing `FILENAME` with the name of the file you downloaded and `LONG CODE` with the long code from the release page — keep the quotation marks — and press Enter:

```powershell
(Get-FileHash "$HOME\Downloads\FILENAME").Hash -eq "LONG CODE"
```

{{#endtab}}
{{#tab name="Linux"}}

Select and copy that **whole line** — the long code, the spaces, and the filename together.

Open a terminal, type this, and press Enter. It points the terminal at your downloads folder:

```sh
cd ~/Downloads
```

Now type this, replacing the words `PASTE THE WHOLE LINE HERE` with what you copied — keep the quotation marks — and press Enter:

```sh
echo "PASTE THE WHOLE LINE HERE" | sha256sum -c
```

{{#endtab}}
{{#endtabs}}

If your browser saved the file somewhere else, use that folder's name in place of `Downloads`.

### 3. Read the answer

On **Mac** and **Linux**, your file is good if the reply ends in `OK`:

```text
startos-0.4.0.1-fdb27c7_x86_64-nonfree.iso: OK
```

On **Windows**, your file is good if the reply is `True`.

> [!WARNING]
> A reply of `FAILED` or `False` means the file on your computer is **not** the one Start9 published. Do not flash it. Delete it, download it again, and check it again. If it fails a second time, stop here and ask on the [Community Hub](https://community.start9.com) before going any further.

Two other replies mean the command went wrong rather than your download. Your file is neither confirmed nor rejected, so correct the command and run it again:

- **`No such file or directory`**, or on Windows a red error above `False` — the command looked somewhere that does not hold your file. Check the folder name and the filename, including its ending (`.iso`, or `.img.gz` for Raspberry Pi).
- **`no properly formatted SHA checksum lines found`** — the line was not pasted whole. Copy it again from the release page, from the first character of the long code through the last character of the filename, and make sure the spaces between them come along.

### Going further: check the signature

The check above proves your file matches what the release page says. A signature proves the file came from Start9 — signed with a key that sits on no Start9 server and never touches the build system, but on a maintainer's own computer. It is the check that still means something if Start9's GitHub account or web hosting were taken over.

It takes longer than the checksum and needs GPG installed: [Gpg4win](https://gpg4win.org) on Windows, [GPG Suite](https://gpgtools.org) on Mac. On Linux you almost certainly have it already.

**1. Get Start9's key from somewhere that is not Start9.**

```sh
gpg --keyserver hkps://keys.openpgp.org --recv-keys 5456DBFF1B9DF905041FA7765259ADFC2D63C217
```

Everything rests on that fingerprint being the right one, so confirm it somewhere else as well. The same key is published at <https://start9.com/start9.gpg>, in the [security policy](https://github.com/Start9Labs/start-technologies/blob/master/SECURITY.md), and in the Debian repository keyring on any machine already running StartOS. The more of those agree, the more sure you can be.

> [!WARNING]
> The download in the next step contains a copy of the key, as `start9.key.asc`. Do not use that one. A key that arrives alongside the signature it is checking proves nothing.

**2. Download `signatures.tar.gz`** from the release page and unpack it, into the same folder as your image. It holds one signature per image, named after the image with `.start9.asc` on the end. A second signature from the person who cut the release is in there too — the `.start9.asc` one is the one to check.

**3. Check your image.** Replace `FILENAME` with the name of the file you downloaded, in both places:

```sh
gpg --verify FILENAME.start9.asc FILENAME
```

**4. Read the answer.** Two things have to be true — the signature is Start9's, and the key it names is the one you confirmed in step 1:

```text
gpg: Good signature from "Start9 <security@start9.com>" [unknown]
gpg: WARNING: This key is not certified with a trusted signature!
gpg:          There is no indication that the signature belongs to the owner.
Primary key fingerprint: 5456 DBFF 1B9D F905 041F  A776 5259 ADFC 2D63 C217
```

- The reply says **`Good signature from "Start9 <security@start9.com>"`**, and
- the **`Primary key fingerprint`** line matches the fingerprint in step 1, character for character.

Those two lines are the answer. The `WARNING` between them is normal and appears every time — it only means you have never personally vouched for the key, which you have no reason to have done.

Anything else is a failure. **`BAD signature`** means the file is not what Start9 signed: do not flash it, delete it, and download it again. If it fails a second time, stop and ask on the [Community Hub](https://community.start9.com). **`Can't check signature: No public key`** means step 1 did not finish — run it again before drawing any conclusion about the file.

## Flash

1. Download and install [balenaEtcher](https://www.balena.io/etcher) onto your Linux, Mac, or Windows computer.

1. Insert your USB drive into your computer.

1. Open balenaEtcher.

1. Click "Select Image" and select the `.iso` image you just downloaded.

1. Click "Select Target" and select your USB drive.

   > [!WARNING]
   > BE ABSOLUTELY CERTAIN you have selected the correct target USB flash drive. Whatever target you select will be **COMPLETELY ERASED**!!

1. Click "Flash!". You may be asked to approve the unusually large disk target and/or enter your password. Both are normal.

## Install

1. Remove the newly-flashed USB drive from your computer and plug it into your server. Choose the fastest available USB 3.0+ port - typically this is blue or labeled "SS" (SuperSpeed).

1. Plug your server into a surge protector — never directly into the wall.

   > [!IMPORTANT]
   > A surge protector is **required** to safely operate your server. Power surges can permanently damage hardware and corrupt your data drive. An [uninterruptible power supply (UPS)](surge-and-ups.md) is strongly recommended for added protection.

1. Power on your server, booting from USB.

   > [!TIP]
   > Some devices do not automatically boot from USB. In these cases, you will need to access your device's BIOS settings and change the boot order to prioritize the USB drive. This is known to be required on the **Nvidia DGX Spark**, among others. You may also need to turn off Secure Boot or explicitly allow USB boot. See the [Community Hub](https://community.start9.com) for device-specific guides or to get help.

1. The StartOS install wizard will now be available at `http://start.local`. You can also use a monitor, keyboard, and mouse. This is known as "Kiosk Mode".

1. Select your language and press Continue.

1. At the Select Drives dialog, select the disk(s) you want to use as your OS and data drives.

1. If a previous StartOS install is detected, it will ask whether you want to Overwrite or Preserve the existing StartOS data. Select Overwrite to start fresh, or Preserve to flash the OS on the booted USB thumb drive over the old installation, while preserving your data. After install is complete, you will be prompted to Continue to Setup.

## Raspberry Pi

> [!IMPORTANT]
> StartOS 0.4.0 supports the **Raspberry Pi 4 ONLY**. Other Raspberry Pi models are not supported.

A Raspberry Pi does not use the USB installer above. Instead, you flash the StartOS image directly to the Pi's microSD card. This is also how a Raspberry Pi is updated to a new major version of StartOS — it cannot update over the air. If you are updating an existing 0.3.5.1 server, complete the [preparation steps in the update guide](update-040.md#prepare-your-server) before flashing.

1. Visit the [Github release page](https://github.com/Start9Labs/start-technologies/releases/tag/start-os/v0.4.0.2) and, from the downloads list, download the **Raspberry Pi `.img.gz`** file.

1. Check it against the release page, exactly as in [Verify your download](#verify-your-download). The release lists a checksum for the `.img` inside the archive as well — the line you want is the one ending in `.img.gz`, because that is the file you downloaded.

1. Insert your Raspberry Pi's microSD card into your computer, using a microSD card reader if needed.

1. Open balenaEtcher (see [Flash](#flash)), click "Select Image" and select the `.img.gz` file you downloaded — there is no need to decompress it first — then click "Select Target" and select your microSD card.

   > [!WARNING]
   > BE ABSOLUTELY CERTAIN you have selected the correct target microSD card. Whatever target you select will be **COMPLETELY ERASED**!!

1. Click "Flash!". When flashing completes, re-insert the microSD card into your Raspberry Pi and power it on.

1. From a computer on the same network, visit [http://start.local](http://start.local) and continue with [Initial Setup](initial-setup.md).
