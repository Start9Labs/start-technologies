# Trusting Your Root CA

In order to establish a secure (HTTPS) connection with your router on the local network, it is necessary to download and trust your router's Root Certificate Authority (Root CA).

> [!NOTE]
> You must repeat this guide for each device you want to connect to the router's web interface over HTTPS.

## Step 1 - Download

Navigate to `System > Settings > General` and click "Download Root CA". This saves the certificate as `startwrt-ca.crt`. When you inspect or install it, the certificate is named "StartWRT Local Root CA" followed by a short random identifier (e.g. "StartWRT Local Root CA 3f8a1b2c") — each router generates a unique one so a reflashed device's new CA won't collide with one you already trust.

## Step 2 - Trust

Select your platform:

{{#tabs global="platform" }}
{{#tab name="Mac" }}

1. Locate your Root CA and double click it. Keychain Access will launch. You will be prompted for your Mac credentials. Select "Modify Keychain".

1. Press Command + Spacebar to launch a program, type in Keychain Access and select the resulting Keychain Access program to open it.

1. Your router's CA certificate will be displayed among the imported certificates in Keychain Access. Right-click on the imported CA cert and select _Get Info_.

1. The details of your CA certificate will be displayed in a new dialog window. Click the "Trust" heading, then select "Always Trust" on **Secure Sockets Layer (SSL)** and **X.509 Basic Policy**.

   Click the red (x) button at the top left of the dialog window.

1. You will then be prompted again for your Mac credentials. Click **Update Settings**.

1. You will see your router's CA certificate as trusted now, signified by a blue (+) sign and the CA cert information will now say "This certificate is marked as trusted for all users" in Keychain Access.

1. If using Firefox, Thunderbird, or Librewolf, complete this [final step](#3-mozilla-apps-firefox-thunderbird-librewolf).

{{#endtab }}
{{#tab name="Windows" }}

1. Click the "Start" menu, type `mmc`, and select "Run as administrator" to access the Windows Management Console. When prompted with the "User Account Control" window, select "Yes" to allow this program to run.

1. When the Management Console opens, navigate to `File > Add/Remove Snap-in`.

1. Select "Certificates" in the left side menu, then "Add". This will open another window.

1. Select "Computer account" and click "Next". Leave defaulted options on the next screen and click "Finish".

1. When you return to the "Add or Remove Snap-ins" page, ensure "Certificates (Local Computer)" exists under "Console Root" in the "Selected snap-ins" section, then click "OK".

1. In the left hand menu of the Management Console, navigate to `Certificates (Local Computer) > Trusted Root Certification Authorities > Certificates`.

1. Right click on the "Certificates" directory, then navigate to `All Tasks > Import`.

1. Click "Next" on the first page of the Certificate Import Wizard, then browse to the location where you saved the downloaded certificate and open it. Then click "Next".

1. On the "Certificate Store" window, ensure that it says "Trusted Root Certificate Authorities" and click "Next". Then click "Finish" on the final screen.

1. Select "OK" when the import is successful.

1. Verify your router's Root CA certificate is in the "Certificates" folder.

1. If using Firefox, Thunderbird, or Librewolf, complete this [final step](#3-mozilla-apps-firefox-thunderbird-librewolf).

{{#endtab }}
{{#tab name="iOS" }}

1. Open your Downloads folder and click on the certificate. It will display a dialog box that says `Profile Downloaded`. Click "Close".

1. Head to _Settings > General > VPN & Device Management_.

1. Under "DOWNLOADED PROFILE", click your Root CA.

1. Click "Install".

1. Click "Install" again.

1. Click "Install" for a 3rd time.

1. You should see green text with a check-mark saying "Verified" under the Profile Installed dialog.

1. Tap "Done".

1. Go to `General > About > Certificate Trust Settings` and enable your Root CA.

1. Click "Continue".

{{#endtab }}
{{#tab name="Android / Graphene" }}

This guide applies to Android 13+, GrapheneOS, CalyxOS, and LineageOS.

1. Go to `Settings > Security > More security settings > Encryption & credentials > Install a certificate > CA Certificate > Install Anyway`, then select your Root CA certificate.

1. If using Firefox, you must use [Firefox Beta](https://play.google.com/store/apps/details?id=org.mozilla.firefox_beta), then complete [this final step](#3-mozilla-apps-firefox-thunderbird-librewolf).

{{#endtab }}
{{#tab name="Debian / Ubuntu" }}

This should work for most Debian-based systems, such as Debian, Ubuntu, Mint, PopOS etc.

1.  Open a terminal and run:

        sudo apt update
        sudo apt install -y ca-certificates p11-kit

1.  Move into the directory where you downloaded your Root CA (usually `~/Downloads`), for example:

        cd ~/Downloads

1.  Add your Root CA to your OS trust store:

        sudo mkdir -p /usr/share/ca-certificates/start9
        sudo cp "startwrt-ca.crt" /usr/share/ca-certificates/start9/
        sudo bash -c "echo 'start9/startwrt-ca.crt' >> /etc/ca-certificates.conf"
        sudo update-ca-certificates

    If successful, you will see the output `1 added`.

1.  If using Firefox, Thunderbird, or Librewolf, complete this [final step](#3-mozilla-apps-firefox-thunderbird-librewolf).

{{#endtab }}
{{#tab name="Arch / Garuda" }}

1.  Move into the directory where you downloaded your Root CA (usually `~/Downloads`), for example:

        cd ~/Downloads

1.  Add your Root CA to your OS trust store:

        sudo pacman -S ca-certificates
        sudo cp "startwrt-ca.crt" /etc/ca-certificates/trust-source/anchors/
        sudo update-ca-trust

    Despite no output from the last command, you can test your app right away.

{{#endtab }}
{{#tab name="CentOS / Fedora" }}

1.  Move into the directory where you downloaded your Root CA (usually `~/Downloads`), for example:

        cd ~/Downloads

1.  Add your Root CA to your OS trust store:

        sudo dnf install ca-certificates
        sudo cp "startwrt-ca.crt" /etc/pki/ca-trust/source/anchors/
        sudo update-ca-trust

    There will be no output if the update-ca-trust command completes successfully.

{{#endtab }}
{{#endtabs }}

## 3. Mozilla Apps (Firefox, Thunderbird, Librewolf)

Mozilla apps use their own certificate store and need extra configuration to trust your Root CA. Complete the steps above for your OS first, then follow the steps below.

For more background, see Mozilla's [blog post on why they maintain their own root certificate store](https://blog.mozilla.org/security/2019/02/14/why-does-mozilla-maintain-our-own-root-certificate-store/).

{{#tabs global="platform-mozilla" }}
{{#tab name="Mac / Windows" }}

1. Open the app and enter `about:config` in the URL bar. Accept any warnings that appear.

1. Search for `security.enterprise_roots.enabled` and set the value to "true".

1. Restart the app.

{{#endtab }}
{{#tab name="Android / Graphene" }}

> [!WARNING]
> The regular Firefox app will not work. You must use [Firefox Beta](https://play.google.com/store/apps/details?id=org.mozilla.firefox_beta).

1. Go to `Menu > Settings > About Firefox` and tap the Firefox icon 5 times to enable "developer mode".

1. Go back to `Menu > Settings > Secret Settings` (at the bottom), and tap "Use third party CA certificates".

{{#endtab }}
{{#tab name="Debian / Ubuntu" }}

1.  In the hamburger menu, click "Settings". Search for `security devices` and select "Security Devices..."

1.  When the Device Manager dialog window opens, click "Load".

1.  Give the Module Name a title, such as "System CA Trust Module". For the Module filename, paste in `/usr/lib/x86_64-linux-gnu/pkcs11/p11-kit-trust.so` and hit "OK".

    > [!TIP]
    > The path to p11-kit-trust.so will be slightly different if your processor's architecture is not x86_64.

1.  Verify that the new module shows up on the left hand side and click "OK" in the bottom right.

1.  Restart the app.

{{#endtab }}
{{#tab name="Arch / Garuda / CentOS / Fedora" }}

No special steps required.

{{#endtab }}
{{#endtabs }}
