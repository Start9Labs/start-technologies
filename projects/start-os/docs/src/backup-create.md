# Creating Backups

Back up your server's data to a physical drive or a network folder.

> [!IMPORTANT]
> Creating backups is an essential responsibility of self-hosting. If you do not make backups, you _will_ eventually lose your data.

## Watch The Video

<div class="yt-video" data-id="omHymkqroRk" data-title="Creating Backups"></div>

## Important Info

1. You can create backups to a [physical drive](#physical-drive) plugged directly into your server, or over-the-air to another device on the same LAN (a [network folder](#network-folder)).

1. Backups are encrypted using your master password. If you change your password prior backups retain the original password.

1. Services may choose to exclude certain files or folders from the backup. For example, Bitcoin excludes the blockchain, since it can be recovered by re-syncing.

1. Backups can take minutes or hours to complete, depending on your hardware and quantity of data.

1. To back up a service, StartOS first stops it (if it was running), performs the backup, then restarts it — but only if it was running beforehand. A service that was already stopped stays stopped. Consequently a service cannot be used while it is backing up, though you may continue to use your server and other services in the meantime.

1. Upon completion, StartOS issues a backup report, indicating which services were backed up, as well as any errors.

1. Backups are differential — each new backup to the same target overwrites the previous one. To maintain multiple backup points, use multiple backup targets.

1. The backup targets list shows the free space available on each drive and network folder, so you can confirm your backup will fit before you start.

1. Backups taken from a specific system architecture (x86, ARM, RISC-V) are backed up for just that architecture. If restored to another architecture, they will likely need to be reinstalled to run efficiently.

1. The backup format changed. New backups are written to a `StartOSBackupsV2` folder on the target, replacing the older `StartOSBackups` (V1) format. StartOS helps you clean up the obsolete V1 data:
   - When you select a target that still holds a V1 backup for this server, StartOS warns you before backing up and shows how much free space remains on the target (see below).
   - After a backup completes, if the target still contains this server's V1 backup, StartOS raises a notification reminding you it is no longer needed.
   - On the `Create Backup` page, any target holding this server's V1 backup shows a **Delete old backup** button — whether or not a new (V2) backup exists yet. After you confirm, StartOS removes this server's old V1 backup to reclaim space; your current `StartOSBackupsV2` backup — and any backups belonging to other servers that share the target — are untouched. If this server has no current (V2) backup on that target, StartOS asks for an extra confirmation first, since deleting the old backup would leave this server with no backup there.

   > [!WARNING]
   > Before backing up, check the free space remaining on the target and make sure your selected services will fit. If space is tight, use the **Delete old backup** button to reclaim the space this server's old backup occupies, or choose another drive.

## Best Practices

Even with proper backups the risk of data corruption is always non-zero. Therefore it is recommended to take additional care when backing up highly valuable or irreplaceable data like a lightning node:

- High quality SSDs should be favored over HDDs as a backup target.
- Backup to multiple targets.
- If backing up to multiple targets make sure all backups are up to date.

## Physical Drive

`EXT4` is the recommended format of your backup drive. `fat32` and `exFAT` are _not_ recommended and may not work.

> [!WARNING]
> Backing up to USB thumb drives or SD card media is not recommended unless you are using high-endurance, high-quality storage. Low-quality flash memory is prone to corruption and failure over time.
>
> If you are using a Raspberry Pi, backup drive _must_ be self-powered, or be connected via a powered USB hub, to prevent possible data corruption.

## Network Folder

A network folder backup sends your encrypted backup over the LAN to a shared folder on another device. First, create a shared folder on the target device, then connect to it from StartOS.

### Step 1. Create a Shared Folder

{{#tabs global="backup-target"}}

{{#tab name="Mac"}}

1. Identify or create a folder to store your server backups.

   > [!TIP]
   > This folder can be located on an external drive connected to your Mac.

1. Go to `System Settings > General > Sharing` and click the "info" icon.

1. Click the toggle to enable file sharing, then click the "plus" icon and select your backups folder.

1. Click "Options" and ensure "Share files and folders using SMB" is checked.

1. Check the box next to the user who owns the folder, then click "Done".

> [!TIP]
> You can find the hostname at the top of the sharing window. The hostname will be an address beginning with `smb://`. To use as hostname, disregard the `smb://` and simply enter the IP address that follows it. Alternatively, you can use the computer hostname (open Terminal and type `hostname`). Either method will work.

{{#endtab}}

{{#tab name="Windows"}}

1. Identify or create a folder to store your server backups.

   > [!TIP]
   > This folder can be located on an external drive connected to your Windows machine.

1. Right click the folder and click "Properties".

1. Click "Sharing".

1. Click "Share".

1. Select a user you want to use for login and click "Share".

   > [!WARNING]
   > If you get a dialog about your network being "Public", you may wish to change to "Private" if this is your home network. Otherwise you may turn on network sharing for public networks.

1. Note the share name shown in the network path (e.g. `\\COMPUTERNAME\SharedFolder` — the share name is `SharedFolder`). You will need this when connecting from StartOS.

{{#endtab}}

{{#tab name="Ubuntu"}}

1.  Install Samba if not already:

    ```
    sudo apt install samba && sudo systemctl enable smbd
    ```

1.  Add your user to samba, replacing `$USER` with your Linux username.

    ```
    sudo smbpasswd -a $USER
    ```

    You will be prompted for your linux password. Then, you must create a new SMB password for the user with permission to write to your new backup share. Keep the password somewhere safe, such as Vaultwarden.

1.  Identify or create a folder to store your server backups. For example:

    ```
    mkdir -p /home/$USER/start9-backup
    ```

    replacing `$USER` with your Linux username and "start9-backup" with whatever you want the folder to be named.

    > [!TIP]
    > This folder can be located on an external drive connected to your Linux machine.

1.  Configure Samba by adding the following to the end of your `/etc/samba/smb.conf` file:

    ```
    [start9-backup]
        path = /home/$USER/start9-backup
        create mask = 0600
        directory mask = 0700
        read only = no
        guest ok = no
    ```

    Where:
    - `[start9-backup]` is the share name — you can change it, but _remember the name_, you will need it later.
    - `path` is the full directory path to the folder you created above.

1.  Restart Samba to apply the changes:

    ```
    sudo systemctl restart smbd
    ```

1.  If your installation of Ubuntu is running a firewall by default or due to your own custom configuration, enter this command to allow connections to Samba. If it generates an error, you can safely ignore it:

    ```
    sudo ufw allow Samba
    ```

{{#endtab}}

{{#tab name="Mint"}}

1.  Install Samba if not already:

    ```
    sudo apt install samba && sudo systemctl enable smbd
    ```

1.  Add your user to samba, replacing `$USER` with your Linux username.

    ```
    sudo usermod -a -G sambashare $USER
    sudo smbpasswd -a $USER
    ```

    You will be prompted for your linux password. Then, you must create a new SMB password for the user with permission to write to your new backup share. Keep the password somewhere safe, such as Vaultwarden.

1.  Identify or create a folder to store your server backups.

    > [!TIP]
    > This folder can be located on an external drive connected to your Linux machine.

1.  Right click the folder and click "Sharing Options".

1.  Select "Share this folder" and give the folder a Share name. _Remember the name_, you will need it later. Click "Create Share".

1.  If your installation of Mint is running a firewall by default or due to your own custom configuration, enter this command to allow connections to Samba. If it generates an error, you can safely ignore it:

    ```
    sudo ufw allow Samba
    ```

{{#endtab}}

{{#tab name="Other Linux"}}

1.  Install Samba if it is not already installed.
    - Arch:

      ```
      sudo pacman -S samba
      ```

    - Debian and Debian-based:

      ```
      sudo apt install samba
      ```

    - CentOS/Redhat

      ```
      sudo yum install samba
      ```

    - Fedora
      ```
      sudo dnf install samba
      ```

1.  Identify or create a folder to store your server backups. Make a note of the directory path. For example:

    ```
    mkdir -p /home/$USER/start9-backup
    ```

    replacing `$USER` with your Linux username and "start9-backup" with whatever you want the folder to be named.

    > [!TIP]
    > This folder can be located on an external drive connected to your Linux machine.

    > [!WARNING]
    > If you are on Fedora 38+, you need to do an extra step to allow the Samba share in SELinux:
    >
    > ```
    > sudo semanage fcontext --add --type "samba_share_t" "/home/$USER/start9-backup(/.*)?"
    > sudo restorecon -R /home/$USER/start9-backup
    > ```

1.  Configure Samba by adding the following to the end of your `/etc/samba/smb.conf` file:

    ```
    [backup-share]
        path = /home/$USER/start9-backup
        create mask = 0600
        directory mask = 0700
        read only = no
        guest ok = no
    ```

    Where:
    - `[backup-share]` can be replaced with whatever you want (must remain inside brackets). This is your `Share Name`. _Remember the name_, you will need it later.
    - `path` is the directory path to the share folder from above.

1.  Open a terminal and enter the following command, replacing `$USER` with your Linux username:

    ```
    sudo smbpasswd -a $USER
    ```

    This creates a password for the Local Network Share. Keep it somewhere safe, such as Vaultwarden.

1.  Enable and restart Samba to apply the changes:

    ```
    sudo systemctl enable smbd
    sudo systemctl restart smbd
    ```

1.  If your Linux system has a firewall enabled by default or due to custom configuration, you may need to allow connections to Samba. The command varies depending on the firewall in use:
    - For systems using UFW (commonly found on Debian-based distros):

      ```
      sudo ufw allow Samba
      ```

    - For systems using firewalld (common on RHEL-based distros):

      ```
      sudo firewall-cmd --permanent --add-service=samba
      sudo firewall-cmd --reload
      ```

{{#endtab}}

{{#tab name="Synology"}}

1. In the Synology UI, go to `Control Panel > Shared Folder` and choose the folder you want to use as the destination for the backup.

   > [!NOTE]
   > Do not select an encrypted folder. Encrypted folders on Synology enforce a character limit of 143 characters. StartOS backups use folder/file names that are longer than 143 characters. The backup process will fail if you try to backup to an encrypted folder.

1. Go to `Control Panel > File Services > SMB` and click the SMB tab if it isn't already selected. Ensure that "Enable SMB service" is checked.

1. Under Advanced Settings on the same tab, set "Min SMB protocol" to `SMB2` and "Max SMB protocol" to `SMB3`.

1. Take note of your device name. Just under "Note" in a pale blue box, you will see "PC (Windows Explorer):" and "Mac (Finder):". These both provide network addresses that contain your device's name. This is the "Hostname" you will need when connecting from StartOS.

1. Still in File Services, click on the rsync tab. Click the checkbox to enable the rsync service.

1. Click "File Station" and locate the desired destination folder. Right click the folder, then _Properties > General_. Next to "Location" will be a folder location. The portion of the location _without the volume label_ is the value you will use for the "Path". For example, if the Location is `/volume1/Backups`, the value you care about is `Backups`.

{{#endtab}}

{{#tab name="TrueNAS SCALE"}}

1. Ensure you have already created a storage pool. If you need help, see the [TrueNAS SCALE documentation](https://www.truenas.com/docs/scale/scaletutorials/storage/pools/createpoolscale/#creating-a-pool).

1. Create a dataset for backups. Go to `Datasets`, select your pool, and click "Add Dataset". Give it a name (e.g. `start9-backup`) and click "Save".

1. Create a user for writing backups. Go to `Credentials > Local Users` and click "Add". Enter a Full Name, Username, and Password. Set Shell to `nologin`. Click "Save".

1. Go to `System > Services`. Enable "SMB" and check "Start Automatically".

1. Go to `Shares` and click "Add" under "Windows (SMB) Shares". Set the Path to your dataset (e.g. `/mnt/pool/start9-backup`). Give the share a name. Click "Save".

1. When prompted to configure the ACL, click "Configure Now". Under "User", select the user you created and set permissions to "Full Control". Click **Save**.

{{#endtab}}

{{#tab name="TrueNAS CORE"}}

1. Ensure you have already created a ZFS pool in `Storage > Pools`. If you need help, see the [TrueNAS CORE documentation](https://www.truenas.com/docs/core/coretutorials/storage/pools/poolcreate/).

1. Create a user for writing backups. Go to `Accounts > Users > ADD`. Enter a Full Name, Username, and Password. Near the bottom, set "Shell" to `nologin` and enable "Samba Authentication". Click "SUBMIT".

1. Go to `Services > SMB`. Enable SMB and check the box "Start Automatically".

1. Open a shell and create your backups directory. For example:

   ```
   mkdir /mnt/zpooldisk1/start9backupshare
   ```

1. Under `Sharing > Windows Shares (SMB)`, click "ADD". Set the path to the directory you created. Give the share a name and click "SUBMIT".

1. A "Configure ACL" dialog will appear. Click "CONFIGURE NOW".

1. On the "Edit ACL" screen, click "Apply User" and select the user you created. Set `Permissions Type` to "Basic" and `Permissions` to "Full Control". Click **SAVE**.

{{#endtab}}

{{#endtabs}}

### Step 2. Connect from StartOS

1. In StartOS, go to `System > Create Backup`.

1. Click "Open New".

1. Complete the form:

{{#tabs global="backup-target"}}

{{#tab name="Mac"}}

1. **Hostname**: The hostname or IP address of your Mac (see the tip in the section above).
1. **Path**: The _name_ of your shared folder, _not_ the full directory path.
1. **Username**: Your Mac user who owns the shared folder.
1. **Password**: Your password for the above user.

{{#endtab}}

{{#tab name="Windows"}}

1. **Hostname**: Your Windows computer name (shown after `\\` in the sharing dialog).
1. **Path**: The share name from the sharing dialog (e.g. `SharedFolder`), _not_ the full directory path.
1. **Username**: Your Windows user who owns the shared folder.
1. **Password**: Your password for the above user. You _cannot_ use a PIN.

> [!WARNING]
> Ensure you are not using a PIN and that Office365 accounts are not in use — try a regular local user instead. If you receive `mount error(115): Operation now in progress`, navigate to `Start > Settings > Network & Internet > Ethernet (or WiFi)` and select the "Private" profile.

{{#endtab}}

{{#tab name="Ubuntu"}}

1. **Hostname**: The name of your Linux machine on the LAN.
1. **Path**: The share name from your Samba config (e.g. `start9-backup`), _not_ the full directory path.
1. **Username**: Your Linux username on the remote machine.
1. **Password**: The SMB password you created with `smbpasswd`.

{{#endtab}}

{{#tab name="Mint"}}

1. **Hostname**: The name of your Linux machine on the LAN.
1. **Path**: The "Share Name" from the sharing dialog, _not_ the full directory path.
1. **Username**: Your Linux username on the remote machine.
1. **Password**: The SMB password you created with `smbpasswd`.

{{#endtab}}

{{#tab name="Other Linux"}}

1. **Hostname**: The name of your Linux machine on the LAN.
1. **Path**: The "Share Name" from your Samba config, _not_ the full directory path (e.g. `backup-share`).
1. **Username**: Your Linux username on the remote machine.
1. **Password**: The SMB password you created with `smbpasswd`.

{{#endtab}}

{{#tab name="Synology"}}

1. **Hostname**: The name of your Synology device on the LAN.
1. **Path**: The _name_ of your shared folder, _not_ the full directory path (e.g. `Backups`).
1. **Username**: Your Synology user who owns the shared folder.
1. **Password**: Your password for the above user.

{{#endtab}}

{{#tab name="TrueNAS SCALE"}}

1. **Hostname**: The name of your TrueNAS device on the LAN (e.g. `truenas.local`).
1. **Path**: The share name you chose when creating the SMB share, _not_ the full directory path.
1. **Username**: The TrueNAS user you created for backups.
1. **Password**: Your password for the above user.

{{#endtab}}

{{#tab name="TrueNAS CORE"}}

1. **Hostname**: The name of your TrueNAS device on the LAN (e.g. `truenas.local`).
1. **Path**: The share name you chose when creating the SMB share, _not_ the full directory path.
1. **Username**: The TrueNAS user you created for backups.
1. **Password**: Your password for the above user.

{{#endtab}}

{{#endtabs}}

> [!WARNING]
> If you receive `Filesystem I/O Error mount error(13): Permission denied`, ensure you have entered the correct values. The hostname can be particularly tricky.
