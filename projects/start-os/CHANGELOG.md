# Changelog

All notable changes to the StartOS OS product are documented here. The format is
based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and StartOS
uses an [extended version](https://docs.start9.com) of semantic versioning.

Full per-release notes are published on the
[GitHub releases page](https://github.com/Start9Labs/start-technologies/releases). This
file tracks notable changes since the move to the monorepo.

## [0.4.0.2]

### Added

- **Typing a service's domain without `https://` opens its web interface over
  HTTPS.** This works on each network where the domain is assigned. Server IP
  addresses and domains assigned to the StartOS UI retain their existing
  behavior. The server's own `.local` name remains reserved for StartOS. See
  [Private Domains](https://docs.start9.com/start-os/private-domains.html).

- **Open UI can honor a service's configured address.** Services that depend on
  one canonical origin can now direct Open UI to that address while it remains
  enabled and compatible with the current browser session.

- **Private-domain routes on a shared port survive a network path that blocks
  PCP.** When you bind a private domain through a Start9 gateway, StartOS asks
  the gateway to route the hostname by TLS SNI — previously only over PCP, so
  a network that filters UDP 5351 between your server and the gateway kept
  ordinary port forwards working while the domain silently stopped being
  routed. StartOS now falls back to asking over UPnP (a Start9 vendor action
  the gateway advertises), so the route comes up either way; PCP remains
  preferred when it gets through.

- **A service can permanently retire a network host or a port it no longer
  uses, and the port numbers it held become available again.** A service that
  reorganizes its interfaces across an update — renaming a host, dropping a
  port — could previously only switch the old one off, which keeps its port
  number reserved for as long as the service is installed. Retiring removes it
  outright and releases its port forwards, proxy entries, local DNS records and
  any port mapping StartOS asked your router for. A domain you had assigned to
  a retired host is removed with it, so check the service's release notes and
  assign it to one of the service's current interfaces.

- **Services that run virtual machines can use the CPU's virtualization
  support.** Packages for emulators, CI runners, and development tools can
  request the hardware interface (`/dev/kvm`) for QEMU, Firecracker, and similar
  workloads. StartOS grants it only to packages that request it and only on
  servers whose processors provide it.

- **A gateway can be marked secure, so services' plaintext addresses are offered
  over it.** `start-cli net gateway set-secure <GATEWAY>` records that you trust
  the network on the other side of a gateway; `unset-secure` hands the decision
  back to StartOS, which trusts only the loopback and container-bridge gateways.
  `net gateway list` shows the current setting. This is one switch for the whole
  server: every installed service's non-SSL addresses — the server's LAN IP
  addresses, its `.local` name and its private domains — are offered on that
  network at once and enabled immediately. An address unlocked this way reaches
  only devices on that gateway's own network segment; it is never opened to the
  public internet. Mark a gateway secure only when you control every device on
  the network it reaches: anything on it can read and alter traffic to a
  plaintext address, including the passwords typed into it. See
  [Gateways](https://docs.start9.com/start-os/gateways.html).

- **An action result that hands you a link can be opened in a new tab.** Where a
  service returns a URL — an authorization link, an admin panel — the result
  shows an open-in-new-tab button beside it.

- **Installed services can be shown as a grid of tiles.** A toggle at the top of
  the page switches between the grid and the list, and your choice follows you to
  any browser pointed at this server. Narrow windows and phones always show the
  grid.

### Changed

- **Your server's name is now its `.local` address, without the `.local` on the
  end.** A server previously carried two names: a display label shown in the
  browser tab, and the `.local` address derived from it by lowercasing and
  hyphenating — so the tab could say "My Cool Server" while every address you
  actually typed said `my-cool-server.local`. There is now one name. The
  `Server Name` field under `System > General Settings` edits the `.local`
  address directly, and accepts lowercase letters, numbers and hyphens — up to 32
  of them, not starting or ending with a hyphen. On update, an existing name
  outside those rules is repaired to fit; every other address stays unchanged.
  The browser tab now shows that address instead of the old display label. **Two `start-cli`
  commands change with it**, so an unattended-install or provisioning script
  needs updating: `setup execute` no longer takes `--name`, and
  `server set-hostname` now takes one required hostname where it used to take an
  optional name and hostname. Setting it moves the `.local` address, where
  passing only a name used to change the label alone.

- **Installing the StartOS UI as an app names it after your server.** Adding it
  to a phone's home screen or installing it from a desktop browser labels it
  with the server name in place of `StartOS`. An app installed before this
  update, or before a rename, may keep the name it was installed with — remove
  it and install it again to pick up the current one.

- **The NVIDIA images now use NVIDIA's open kernel modules, which support GeForce
  RTX 20-series, Quadro RTX and newer.** This is what makes current cards work at
  all — an RTX 50-series, an RTX PRO 6000 or an NVIDIA GB10 can only be driven by
  these modules. The trade is at the other end of the range: **a GeForce GTX
  900-series or 10-series card, a Titan X or Xp, or a Tesla M40, P40, P100 or
  V100 will no longer be driven by the NVIDIA images.** If you rely on one of
  those, stay on 0.4.0.1 or use the Standard image, whose open-source `nouveau`
  driver still provides display output without GPU compute.

### Fixed

- **The port-forwarding test reports a port as open to the Internet only where
  it is reachable from the Internet.** Where StartOS's port-forward request was
  granted by a router that sits behind another router, the test could pass a
  port that nothing outside could reach, and two otherwise identical setups
  could disagree depending on which forwarding protocol the router spoke.
  StartOS now measures the port from the Internet in that case and reports
  what it finds.

- **Client connections through StartOS's TLS-terminating reverse proxy now fail
  within 15 seconds if StartOS cannot connect to the service or complete a
  required TLS handshake with it.**

- **Transfers preserve the source filesystem format.** StartOS mounts source
  filesystems read-only while copying persistent data, repairing ext4 only when
  needed to mount it. This leaves the source drive available as a fallback.

- **An app that remembers your server's certificate sees the same certificate
  across every route to that name.** Wallets and other apps that pin the first
  certificate they are shown — Sparrow and the Electrum clients most visibly —
  raised a man-in-the-middle warning when the same name resolved through a
  different address or the server's public IP changed. StartOS now reuses one
  certificate per name until renewal.

  This changes how one address behaves, on a server reached through a NAT
  router: typing its public IP address while on the same network as the server
  now produces a certificate warning, because the router rewrites such a
  connection to the LAN address and the server cannot see which of the two you
  asked for. Reach it from inside your own network by its `.local` name, by a
  domain you have assigned to it, or by its LAN IP address. A server that holds
  its public address directly, or that is reached over StartTunnel, is
  unaffected. See [Public IP](https://docs.start9.com/start-os/public-ip.html).

- **A downgrade to a version that cannot take over the service's data is refused
  before anything is downloaded or stopped**, with an explanation of what to do
  instead.

- **A service that fails to install, update, restore or uninstall says so right
  away.** The notification naming what went wrong was held back until StartOS had
  finished cleaning up after the attempt, which can take several minutes. It now
  arrives as soon as the operation fails, while that cleanup is still running.
- **Restoring from a backup, or transferring to a new drive, keeps your server's
  name.** Both flows renamed the server to `start9`, so the restored server
  answered at `start9.local` rather than the address it had before — and two
  servers restored on the same network collided on that one name.

- **Apps that hold a connection open — desktop sync clients, API pollers — stop
  dropping in and out.** Nextcloud Desktop and clients like it showed a
  recurring "Network error" that cleared itself a few seconds later. A service
  routinely closes a connection once it has sat idle for a few seconds, and a
  client is built to notice that and open a fresh one. StartOS's reverse proxy
  kept the client's half of the pair open after the service had closed its own,
  so the client went on holding a connection it had every reason to believe was
  good, and the next request it sent over that connection was cut off with no
  reply. The proxy now closes the client's half as soon as the service closes
  its own, which is the signal the client is waiting for.

- Trim whitespace on form inputs.

- **Marketplace search matches anywhere in a package's name, ID or
  description, and ranks results by how closely they match.** Searching
  `cloud` finds Nextcloud.

- **Restoring a service from a backup is many times faster.** A restore reads
  the service's whole package out of the backup, and the encrypted backup
  filesystem stores file contents as sealed 1 MiB blocks that it fetches and
  decodes whole. It kept none of them, so each 8 KiB read along the package
  fetched and decoded the enclosing block over again — dragging roughly 72 GB
  across a network share to deliver a 480 MB package, and turning a restore
  that should take a couple of minutes into 42. A block is now decoded once
  for the run of reads inside it, which for reading a file start to finish is
  once per block.

- **A service that reads back a region of a file it has only partly written
  no longer fails its backup.** Inside a backup or restore, a service's own
  procedure works against the encrypted backup filesystem directly, and a
  program that writes at one offset and then reads a little further on —
  a database extending a file, an image being assembled — asked for bytes
  that the pending write had not reached. That read failed with an I/O
  error, and closing the file afterwards left the whole backup filesystem
  unresponsive, so the operation hung rather than finishing. Such a read
  now returns the zeros it should.

- **File operations within a backup handle end-of-file and partial failures
  correctly.** Reads past the end return no bytes, large copies use bounded
  memory, and a copy interrupted by an error reports the bytes it completed.
  A failed write during a storage-layout change preserves a readable inode,
  its length, and every successfully written prefix byte.

- **Helper processes a service starts are cleared away once they finish.** A
  service that shells out to other programs — a media downloader calling
  `yt-dlp` and `ffmpeg`, an agent running tool subprocesses — orphans a helper
  whenever the program that started it exits first. Those finished helpers
  stayed listed inside the service's container as `<defunct>`, each still
  holding a process slot, for as long as the service ran — so a service that
  starts many of them built them up without limit, and only a restart cleared
  them. The container's first process now collects them as they finish.

- **A Let's Encrypt certificate is issued even when the authority takes longer
  than a second to check your domain.** StartOS asked Let's Encrypt to run the
  check and then, one second later, asked it to run the check again — which
  Let's Encrypt refuses, because the one it was already running had not
  finished. The whole request failed there, and the retry a minute later failed
  the same way, so a domain could sit without a certificate indefinitely. Let's
  Encrypt validates from several vantage points around the world and routinely
  takes longer than a second, so this affected almost every domain. StartOS now
  waits for the answer instead of asking again.

- **A Let's Encrypt domain works on an interface served on a port other than
  `443`** — an Electrum server on `50002`, a TURN server on `5349`. Let's
  Encrypt proves you control a name by connecting to it on port `443` whatever
  port the service behind it uses, and nothing on your server answered there for
  that name: the certificate was never issued, so the address served your
  server's Root CA instead and clients that validate against public authorities
  could not connect at all. StartOS now answers the challenge on port `443` for
  every domain it holds an authority for, and claims that port automatically
  wherever the domain is public — over StartTunnel, and as an IPv6 firewall
  pinhole on a gateway that opens one. On a router, forward `443` to your server
  as you would for a standard domain — the domain's own port is not enough on
  its own, and Port Forwards lists the `443` rule alongside it. The checks that
  run when you add or enable such a domain cover port `443` as well, so a domain
  that would refuse every connection is reported before you go looking for it,
  with its own row in the setup modal. Such a domain is served on its own port
  only, so reach it as `https://<name>:<port>`; `443` carries the certificate
  authority's checks and nothing else.

- **A service that presents its own TLS certificate is shown as presenting
  it.** StartOS does not terminate such a connection, so the certificate the
  client checks is the service's — but a domain on that interface reported
  whichever certificate authority had been chosen for it. Those addresses now
  report `Self signed`.

- **Services that run their own containers or a VPN can reach the devices they
  were granted.** A service opting into `userspaceFilesystems` or
  `virtualNetworking` is handed `/dev/fuse` and `/dev/net/tun` inside its
  container. Those nodes are now created with the same permissions the host
  gives them, so a service running as a non-root user can open them. Previously
  the permissions were left to whatever StartOS's own file-creation mask
  produced, and only the container's boot-time device pass — which races with
  the grant — widened them; on the losing side of that race a CI runner's jobs
  failed to start with `Failed to open() /dev/net/tun: Permission denied`.

- **Reinstalling with "Preserve" copies your server's configuration before it
  rewrites the drive, so the configuration survives.** The copy used to be taken
  afterwards, by which point there was nothing left to read — so the server came
  back with its configuration reset, and said nothing about it. Among the things
  lost was the key your server uses to sign its add-on drivers, which on a
  machine with Secure Boot left hardware such as an NVIDIA GPU unavailable
  afterwards. A reinstall that cannot read the configuration it was asked to keep
  now stops and says so, rather than continuing and discarding it.

- **On a server with Secure Boot enabled, hardware that needs an add-on driver —
  an NVIDIA GPU, most commonly — works after setup.** Secure Boot only loads such
  a driver once you approve the key your server signs it with, and approving that
  key is protected by your master password. Your server only ever asked the
  firmware to trust the key while starting up, which on a new server happens
  before you have set a password, so the request was never made and you were
  never shown the prompt — leaving the driver unable to load, with nothing to say
  why. Your server now asks as soon as you set your password, so the prompt
  appears on the next restart. See
  [Initial Setup](https://docs.start9.com/start-os/initial-setup.html).

- **The 64-bit ARM NVIDIA image boots on NVIDIA GB10 hardware again, such as the
  DGX Spark, GPU workloads like Ollama and vLLM run on it, and the image is
  considerably smaller.** Four separate faults were involved, two of which each
  stopped the machine on the last line the bootloader printed — so fixing either
  one alone changed nothing. These images drive the GPU with NVIDIA's own driver
  and turn nouveau off, yet were still built with graphics firmware they cannot
  use — nouveau's, for every chip it supports, along with an older driver
  series' — and 152 MB of it sat inside the initramfs that the bootloader must
  read in full before the kernel starts. Separately, a Tegra fabric driver that
  recent kernels build in claims one of the GB10's internal buses and reads a
  protected register on it, stopping the machine during hardware detection before
  it can display anything. Third, the image built NVIDIA's driver in the flavour
  that does not support this generation of GPU, so the driver found the card but
  could never bring it up. Last, the GPU came up and reported itself correctly
  while every CUDA program still failed at startup, because NVIDIA's driver
  declines a GPU whose address-translation support does not match what the kernel
  offers, and this kernel is built without the support that an integrated GPU
  like the GB10 advertises. The images now carry only the graphics firmware they
  can use, switch that driver off, build the kernel modules NVIDIA supports here
  — which is what NVIDIA's own operating system does on the same hardware — and
  accept the GPU rather than turning it away.

- **Your server answers to its own addresses and no others.** A name that
  resolved to your server but was never configured on it — a domain you pointed
  at its LAN IP, or its `.local` name typed without the `.local` — was served
  your dashboard, along with a certificate for that name signed by your server's
  Root CA. Logging in was never possible under those names, so the page could
  not be used for anything, but it should not have been reachable. Your server
  now serves its `.local` address, the domains you have assigned to it, and
  direct connections to its IP address.

- **Image upgrades verify their checksum again.** `upgrade` compared the image's
  blake3 hash only when it was given a second positional argument, which no
  caller passed — so the comparison never ran and a corrupt but still mountable
  image would be installed without complaint. It now verifies whenever
  `CHECKSUM` is set.

- **Large QR codes render, and a value too long for any QR code says so rather
  than opening an empty dialog.** Copy that value instead of scanning it.

- **Removing a domain from a service leaves its network settings otherwise
  untouched.** Naming a network host the service does not have — a stale id, or
  a typo — added that host to the service as an empty entry, which then stayed
  in its network settings with nothing to remove it.

- **A service whose startup routine throws now reports the failure in its own
  logs, and StartOS names the failure for what it is.** The container runtime
  handed the exception back to StartOS over its socket without also printing
  it, so a service that failed to start went quiet in `Logs` at the moment it
  needed to speak, while restarting every ten seconds. The exception now
  appears in the service's own log next to the procedure that raised it, and
  StartOS labels a failure that came from the runtime `Service Runtime Error`
  rather than `Unknown Error`.

- **The StartOS UI is served over plain HTTP on port 80.** Servers set up before
  0.4.0.1 gave the interface a high-numbered port instead, and nothing answered
  on it — so a service that reached the StartOS API over the container bridge,
  and any address StartOS reported for its own plaintext interface, pointed
  somewhere dead. Existing servers move to port 80 on update, and the high port
  goes back to the pool for services to use.

- **The login banner reports system status for every user, not just the first
  one to log in.** It staged its database snapshot at a fixed path in `/tmp`,
  which `pam_motd` created as root at login — so any subsequent non-root run
  could not write it and the banner fell back to `Services: Unknown`,
  `WAN: N/A` and `NTP: Unknown`. It now uses a private temporary file and
  removes it on every exit path.
- **A failed or cancelled service update can no longer lose that service's data.**
  Rolling an update back replaces the service's data with the copy taken before the
  update started. That replacement used to delete the current data before putting the
  copy in place, so an interruption in between — a restart, a power loss, or a second
  failure — could leave the service with neither, and the next update attempt would
  discard the surviving copy as stale. The rollback now moves the current data aside
  and only drops it once the copy is fully in place, so an interruption at any point
  leaves a state StartOS can finish on the next boot, and a rollback that cannot be
  completed is reported instead of passing silently. A failed first-time install over
  data that was already there no longer deletes that data either.
- **The copy taken before an update is now made with the service stopped**, so it can
  no longer capture a database mid-write.

- **A service reached over IPv6 through a tunnel now answers.** StartOS sends a
  reply back out the interface its connection arrived on by restoring a
  connection mark, but the kernel routes the reply that _opens_ a connection
  before that mark is restored. On a server whose gateway carries no IPv6 of its
  own, that reply fell to the gateway's routing table — which drops IPv6 to keep
  it from leaking out the wrong interface — and was discarded before it was ever
  sent, so an inbound IPv6 connection to a tunnel-delegated address hung until
  it timed out. A reply from an interface's own global IPv6 address now leaves
  by that interface. IPv4, and traffic forwarded to a service container, were
  unaffected.

- **Notification selection checkboxes no longer cover text on phones.** When
  notification selection is active, each checkbox replaces its notification
  icon while preserving the title's spacing.

- **A service that uses UDP is reachable from the Internet on a public IP
  address.** StartOS already passed UDP through to the service, but the mapping
  it asked your gateway for over PCP, NAT-PMP or UPnP covered TCP only, and a
  router maps each protocol separately — so a VPN, a video-call relay or a game
  server needed a UDP rule added to the router by hand. StartOS now asks for
  both protocols: on a public IP address, over IPv4 and IPv6 alike, and on a
  published port range. It withdraws both when the address is disabled or
  deleted.

- **An interface a service update moves to a different internal port is listed
  once, not twice.** StartOS keeps a service's former port binding dormant so
  its addresses and external port survive if the service returns to it — but
  the interface record exported from that binding lingered too, so after an
  update like Jitsi's (Web UI moved from port 80 to 8000) the service showed
  the same interface twice. Exporting an interface now removes the record of
  its previous export, wherever it lived; a lingering duplicate clears the next
  time its service initializes — at the reboot this update performs.

- **A port a service asks StartOS to keep off insecure networks stays off them,
  even when the service also publishes a port range.** A range and a single port
  could both claim one container port, and a range carries its own exposure
  rules — so the range's rule could reach the port on gateways the port's own
  settings excluded. StartOS now rejects the overlapping claim and names both
  ports, so the service reports the conflict instead of serving it.

- **An interface whose service encrypts its own end now loads when the binding
  pins the protocols it offers.** Setting `alpn` changed how StartOS dialled
  the service as well as which protocols it put forward, so the service
  received plaintext on a port expecting TLS. The setting now narrows only the protocols on offer —
  the service chooses among the ones the client also asked for, and both ends of
  the connection carry the one it chose.

- **A service that speaks HTTP/2 to StartOS now loads in a browser.** StartOS
  opens a fresh TLS connection to a service that encrypts its own end, and
  offers it the client's own list of application protocols. Where the service
  chose HTTP/2 from that list, every request on that connection ended in a
  connection error. The client's own connection had been offered nothing and
  settled on HTTP/1.1, so StartOS wrote HTTP/1 requests onto a connection the
  service was reading as HTTP/2. StartOS now offers the client exactly the
  protocol the service chose, so both halves of the connection carry the same
  one. For HTTP/2, StartOS advertises WebSocket extended CONNECT when the
  service includes support in its opening settings. If the service connection
  ends, StartOS sends GOAWAY so the browser follows HTTP/2's orderly shutdown.

- **Dependency releases satisfy one complete version-range branch.** A release
  may use an installed or aliased version, but one version must satisfy every
  term in a conjunction. An exclusion rules out its branch when a declared
  version satisfies the complete excluded range. Dependency warnings, update
  checks and marketplace filtering now agree on that evaluation.

- **Versions of different flavors sort in a stable order.** Comparing across
  flavors produced no answer, so a list mixing Bitcoin Core with Bitcoin Knots
  kept whatever order it arrived in.

### Security

- **Service mount paths are validated and confined to their intended
  directories.**

- **An address you assigned to a public certificate authority serves that
  authority's certificate and nothing else.** When StartOS could not obtain the
  certificate, the address fell back to one signed by your server's Root CA
  while still listing Let's Encrypt as its authority — so the fallback was
  invisible from the server itself, whose own trust store contains that Root CA.
  Your Root CA chain is the same on every address your server exposes, which
  makes it a value that links them all to one server, handed to anyone who
  connects. Such an address now refuses the connection until its certificate is
  available, and tells you so: a notification names the domain and why issuance
  failed, once per domain until a certificate lands. A certificate already
  issued keeps being served through its last 30 days while renewal is retried,
  so a renewal that begins failing does not take the address down, and a domain
  you also reach on your local network keeps answering there with your server's
  own certificate.

## [0.4.0.1]

### Changed

- **A service that serves its own TLS certificate now reports its external port
  as an SSL port, and StartOS serves it like one.** Every interface whose
  external port speaks TLS — whether StartOS terminates it or the service
  presents its own certificate — now carries that port in `assignedSslPort`,
  and `assignedPort` means a plaintext port. A self-TLS port is now answered by
  the StartOS SNI router, which pipes the raw TLS stream to the service with
  the client's address preserved, instead of a kernel port-forward — so every
  TLS-carrying port behaves uniformly, and a self-TLS service's domains are
  advertised on its preferred port (e.g. 443) exactly as when StartOS
  terminates TLS. This also means such a port accepts TLS connections only,
  and no longer relays UDP. The port number itself is
  unchanged, so existing addresses, bookmarks and router port-forwards keep
  working. Packages resolve a dependency's address with
  `sdk.host.getBridgeAddress`, which is correct under either arrangement; see
  [Service-to-Service Networking](https://docs.start9.com/packaging/service-to-service.html).

- **The StartOS web interface holds ports 80 and 443.** StartOS runs as root, so
  its own interface is the one binding that may claim the privileged range, and
  it now does so through the same port allocator every service uses. HTTPS was
  already served on 443; the plaintext address — offered only over loopback and
  the service bridge — moves from a random high port to 80.

### Fixed

- **Updating from 0.3.5.1 starts the Tor service it installs.** Your existing
  onion addresses come across with it and answer as soon as the update
  finishes, with nothing to start by hand.

- **A service that fails to convert while migrating from 0.3.5.1 reports the
  reason.** The v1→v2 package conversion raises a notification against that
  service carrying the error that stopped it, in the same form as an install
  failure — alongside the summary notification listing every service to
  re-install.

- **A service that is renamed during migration is recorded as migrated.** Ghost,
  Synapse, Monero, Nostr and Fedimint are installed under new ids on 0.4.0
  (`ghost-legacy`, `synapse-legacy`, `monerod-legacy`, `nostr-rs-relay` and
  `fedimint-guardian`), and the migration looks each one up under the id it was
  installed as. These services complete their migration without appearing among
  the failures, and the failure list names services by ids that exist in the
  marketplace.

- **The over-the-air update to 0.4.0 boots on the Server Pure.** The Server
  Pure's PureBoot firmware reads the boot configuration itself rather than
  running GRUB, and it takes the kernel and initramfs paths literally. The
  update now writes those paths in the plain form PureBoot expects, so the
  server boots into 0.4.0 on the restart that applies the update.

- **A Server Pure applies its PureBoot firmware update.** StartOS installs the
  firmware image at the path it reads it from, so a Server Pure on an older
  PureBoot release updates its firmware on the next start.

- **Installing onto a pre-installed Raspberry Pi keeps the data pool you pick.**
  Selecting the data pool by partition path now preserves that choice through
  installation.

## [0.4.0]

The stable release of StartOS 0.4.0, ending the 0.4.0 early-access program. No
0.4.0-beta.10 was ever cut — the changes that had accumulated under that heading
ship here instead — so this entry covers every change since 0.4.0-beta.9, the
last released beta.

### Added

- **Automatic `.local` resolution over WireGuard gateways.** StartOS now injects
  a DNS record for its own `<hostname>.local` name into every connected WireGuard
  gateway (e.g. StartTunnel) — and only those gateways — via RFC 2136. Clients on
  the tunnel can't resolve `.local` by mDNS (Android and others exclude VPN
  connections from mDNS), so without this a `.local` lookup goes to the tunnel's
  resolver and fails; now the tunnel answers it, and the name resolves from
  anywhere the tunnel reaches. Once a gateway's resolver accepts the record,
  `.local` is listed among that gateway's addresses in the UI, like on the LAN.
  The gateway's own DNS-injection policy still applies — a gateway with
  injection disabled refuses the record, is not listed, and `.local` there
  falls back to a manual DNS record — and plain LAN gateways are left
  untouched, since mDNS already works on the LAN. A gateway whose resolver
  refuses or times out is backed off on the same trust-window cadence as
  port-mapping probes, not retried every few minutes.
- **IPv6 GUA exposure control.** On a service interface, an IPv6 global-unicast
  address (GUA) keeps the usual on/off toggle and adds a **Local / Public**
  dropdown in the access column. **Local** (the default) keeps it reachable on
  the local network only — traffic from outside the subnet is rejected;
  **Public** exposes it to the Internet and attempts an automatic gateway
  pinhole (PCP). The choice is carried by the address's `public` flag, so
  services selecting addresses for P2P see the correct reachability. A GUA on a
  StartOS-terminated SSL port is served by the host's own TLS listener; a port
  StartOS does not terminate is forwarded (DNAT) directly to the service
  container. IPv6 ULAs and IPv4 are unchanged.
- **DualStack public domains.** A public (clearnet) domain is now reachable over
  both IPv4 and IPv6 whenever its gateway has an IPv6 global-unicast address
  (GUA). StartOS advertises an `AAAA` target (the GUA) alongside the `A` target,
  opens an inbound IPv6 firewall pinhole for the domain's port (via PCP — v6 is
  NAT-free, so there is nothing to forward), and serves the domain on the GUA
  through its existing SNI-routed TLS listener. The bare GUA itself is not
  exposed — only traffic matching the domain by SNI is accepted — so an SSL
  domain needs no separate GUA WAN opt-in (a plaintext domain, which has no SNI
  to filter on, exposes the GUA the same way it exposes the WAN IPv4). The
  add-domain DNS check and the domain setup modal now verify and display both
  the `A`/IPv4 and `AAAA`/IPv6 records and reachability; IPv6 reachability is its
  own `net.gateway.check-port-v6` endpoint (separate from the IPv4
  `net.gateway.check-port`) so each family is probed independently.
- **`--force` on service start.** `start-cli package start <id> --force` (and the
  `package.start` RPC `force` flag) starts a service even when it has an unresolved
  critical task.
- **Automatic gateway configuration (#3306).** A StartOS server now opens its own public ports and publishes its private-domain DNS by talking to its gateway — a home router or a StartTunnel — instead of leaving it as a manual step.
  - **Automatic port forwarding.** When a public address needs a port open, StartOS opens it by speaking a port-control protocol to the gateway: PCP (RFC 6887) → NAT-PMP → UPnP IGD, in that order. Mappings are reference-counted and withdrawn when the address is disabled or deleted; each active mapping is renewed at about half its gateway-granted lease, well before it would expire, so a still-wanted forward is never dropped, and letting a mapping lapse (no further renewal) is itself a teardown path once an exposure goes away. A single shared `PortMapController` also answers reachability, so `check_port` skips the remote echo probe when an automatic mapping is already active and reports the gateway-assigned external IP directly. Port mapping is scoped to the gateway each exposure actually routes through (the interface's own subnet gateway), so the box never probes an unrelated LAN router. A port's forwards and mappings are driven only by the addresses advertised at that exact port — exposing an SSL address never opens its plaintext sibling.
  - **Private-domain DNS injection (RFC 2136).** When a private domain is enabled on a gateway, StartOS pushes an `A` record (domain → this host's IP on that subnet) to the gateway's DNS server via DNS UPDATE so LAN devices that don't use StartOS's resolver can still resolve it, and withdraws it on disable/delete. `check_dns` now verifies a private domain by resolving the specific FQDN against the LAN's DNS server(s) and confirming it returns one of this server's LAN addresses.
  - **PCP HOSTNAME extension (SNI demux).** SSL/TLS services can share a single external port (443) across many hostnames: the gateway demultiplexes inbound TLS by SNI. StartOS emits PCP HOSTNAME mappings for public-domain vhosts (PCP-only — NAT-PMP/UPnP can't demux), gated by a PCP ANNOUNCE capability probe so the option is only sent to gateways that understand it. The protocol is documented as an Internet-Draft (`rfcs/draft-start9-pcp-hostname`).
  - **PCP PORT_SET (RFC 7753).** A contiguous port range maps in a single PCP MAP instead of per-port sweeps.
  - **Per-gateway capability tracking, sharding, and backoff.** Each gateway's PCP/NAT-PMP/UPnP support (and the PCP HOSTNAME extension) is recorded on the gateway in the database — fed by periodic watcher probes and by the outcome of every mapping attempt — so a gateway that refuses a protocol stops being asked instead of paying a timeout per attempt (a yes is trusted for an hour, a no re-probed after five minutes). Mapping work is sharded per gateway interface, so one gateway's slow or absent answers never delay another gateway's mappings, and a mapping that keeps failing backs off exponentially (15 seconds, doubling to a 16-minute cap) rather than retrying on a fixed interval.
  - **In-place WireGuard gateway updates.** `start-cli net tunnel update <id> <config>` (and a per-gateway **Update config** UI action) re-issues a WireGuard config onto the existing interface without churning the gateway identity, so forwards and public/private domains keyed to it survive the swap — primarily to add a `DNS =` line to an existing config. The update path uses NetworkManager `Update2` + `Device.Reapply`, so updating the gateway carrying the request no longer drops its own transport. Because NetworkManager (≤ 1.52) strips WireGuard peer preshared-keys on `Reapply` even when they are passed to it explicitly, StartOS re-applies each peer's PSK straight to the kernel device (over stdin, never argv) after the reapply, so the tunnel keeps its shared secret.
  - **Best-effort HTTP→HTTPS redirect (IPv6).** When a service is publicly exposed on 443 over an IPv6 GUA, StartOS asks the gateway for an `80→443` redirect pinhole so plain `http://` auto-redirects to `https`. Over IPv4 no such map is requested — the upstream gateway (e.g. StartTunnel, which serves a port-80 redirect by default) handles it.
  - **Insecure exposures never reach the WAN.** A port is opened to the public internet (an IPv4 WAN forward, an IPv6 GUA pinhole/forward, or an upstream port-map) only when the exposure is itself secure — TLS on the wire or a self-securing protocol. A plaintext exposure can still reach the LAN over a gateway explicitly marked secure, but the WAN is treated as never secure regardless of the gateway's setting.
- **Private domains on StartTunnel gateways (ba5396f49).** Now that a StartTunnel proxies DNS per subnet, a private domain resolves for tunnel clients (their DNS forwards to this server, which answers with the gateway's tunnel address), so the UI no longer restricts the public/private domain picker to router gateways — all gateways can use it. The "DNS Server Config" guidance is now gateway-aware: for a tunnel gateway it instructs you to point the StartTunnel subnet's DNS at this server. The private-domain and clearnet setup dialogs also surface the automatic alternatives (enable DNS Injection for the device; automatic UPnP/NAT-PMP/PCP port forwarding) instead of describing only the manual paths.
- **`MultiHost.bindPortRange` backend (#3270).** Host support for reserving a contiguous TCP+UDP port range (2–500 ports) in one call, stored as a single `RangeBindInfo` record under `Host.binding_ranges` and installed as one nftables rule per chain (via `PortForward.count`). Intended for real-time / WebRTC servers (coturn, RTP, SIP). Backed by the new `bindRange` effect.
- **Package init progress reporting (#3323).** A service can stream progress during the install/update finalization phase via the new `setInitProgress` effect; the host nests it inside the install's finalization phase using the standard `FullProgress` wire format, and `setupInit` auto-reports one step per composed init handler.
- **Phased backup progress (#3250).** `serverInfo.statusInfo.backupProgress` now uses the standard `FullProgress` shape (matching update/install progress). `NamedProgress.progress` is generalized to `PhaseProgress` so a phase can carry sub-phases, and the new `setBackupProgress` effect lets a service container stream its own backup sub-progress.
- **Backup format v2 (#3289).** Backups are written to a new `StartOSBackupsV2` directory, and the backup report now includes per-package duration. The backup targets list shows the free space available on every drive and network folder so the user can confirm a backup will fit before starting it. When this server's pre-v2 `StartOSBackups/<server-id>` backup is present on the selected target, the UI additionally warns before backup, then confirms the format change (#3324, `backup.target.legacy-info` RPC). Once migrated, StartOS also helps remove the now-obsolete V1 data: after a backup completes, if the target still holds this server's V1 backup, a warning notification reminds the user it is no longer needed; and the backup-create page shows a **Delete old backup** action for any target holding this server's V1 backup — whether or not a new (V2) backup exists yet — removing only this server's old backup; other servers' backups and `StartOSBackupsV2` are untouched (`backup.target.delete-legacy` RPC). Deletion returns immediately — the backup is atomically moved to a hidden trash folder on the target — and its space is reclaimed by a background sweep that posts a notification when it finishes (unlinking a large backup can take hours on some filesystems); a backup started while reclamation is pending finishes it first, shown as a `Reclaiming Space` phase in the backup's progress. Deleting requires a confirmation, plus an extra confirmation when this server has no current (V2) backup on the target (so the user can't unknowingly delete their only backup). Legacy detection and deletion are scoped to the current server's ID, so a target shared by several servers no longer flags or removes another server's backup.
- **Direct cross-package action runs via `access` (#3267).** Action metadata gains an `access` field (`'public' | 'dependent' | 'user'`, default `'user'`) controlling who may invoke an action directly through `effects.action.run`. Public/dependent actions give dependents a direct path instead of only creating a task; direct runs still honor the action's `visibility` and `allowedStatuses`.
- **`input-not-matches` tasks accept multiple values (#3310).** `TaskInput` splits into `accept` (a list of acceptable partial inputs) and `set` (the value to prefill when none match); the cross-package critical-conflict guard fires only when the input conflicts with every `accept` entry. The host still accepts the legacy `{ value }` shape over the effects socket for s9pks built on the pre-2.0 SDK.
- **Service interfaces tab.** Service interfaces are promoted to a dedicated sidebar tab; the dashboard interfaces card and per-interface detail route are removed, sidebar nav labels are decoupled from route paths, and the tasks table is redesigned (action-first, service rendered as an icon).
- **Unified marketplace + brochure.** The in-OS marketplace and the public brochure now share `@start9labs/marketplace` components behind an `AbstractMarketplaceService` (the OS persists to patch-db, the brochure to localStorage), so both ship the identical detail/preview UI. The brochure app is ported into the workspace and auto-deployed to `marketplace.start9.com` on master. The registry-selection modal is replaced by an inline registry-select dropdown (switch/add/delete inline), and custom registries can now be added by bare domain (https default; http for `.onion`) (#3349). Empty categories are hidden (snapping back to "all" on a registry switch that empties the selection), category icons are refreshed for the current set, and a "Package a service" link sits beneath the sidebar categories.
- **Nested idmapped mounts (#3248).** StartOS gains syscall-based mount primitives (`open_tree`/`move_mount`/`fsopen`/`mount_setattr`) and a `start-container mount` path, wiring up the SDK's `idmap` field on volume/asset/dependency/backup mounts end-to-end. See [`../start-sdk/CHANGELOG.md`](../start-sdk/CHANGELOG.md) for the SDK-facing surface.
- **Raspberry Pi image hardening (#3249).** Vendor kernel bumped to 6.18.33+rpt with apt pins so `/boot` stays vendor-only, `earlycon` for first-boot diagnostics, a loop-safe self-diagnosing `init_resize`, and data-drive-only setup for pre-installed devices.
- **Graceful shutdown on external power events (#3319).** Two systemd pre-shutdown barrier units (`startos-shutdown.service` / `startos-restart.service`) call `start-cli server shutdown/restart` and wait for graceful container teardown, so externally-initiated shutdowns (UPS / `qm` / ACPI) tear services down cleanly. The shutdown/restart RPC gains an opt-in `wait` param.
- **iOS root-CA install via configuration profile (#3240).** A new endpoint serves an unsigned Apple Configuration Profile (`PayloadType com.apple.security.root`); iOS/iPadOS download links are UA-sniffed and routed to `.mobileconfig`, fixing the broken `.crt` install flow on iOS 26.5 Safari.
- **`diagnose-hang` capture script (#3236).** Captures startd runtime state entirely via `/proc` and basic tools (per-thread kernel stacks, fds/sockets, journal tail, dmesg, disk health, lxc status) when startd is unresponsive and `start-cli` can't help.
- **`lo` and `lxcbr0` treated as secure networks (#3297)** for insecure (plain-HTTP) traffic, since loopback and the container bridge never leave the host; an explicit secure setting still overrides the intrinsic default.
- **In-place 0.3.5.1 → 0.4.0 update path.** Updating the OS from 0.3.5.1 no longer
  requires syncing a whole root filesystem file-by-file — the step that grew
  flakier the more files a device had. StartOS 0.3.5.1's existing over-the-air
  updater instead receives a compact migration payload (the 0.4.0 base image plus
  a boot-time rewire) and the 0.4.0 initramfs converts the on-disk layout to the
  0.4.0 format on first boot. The data partition is preserved and the existing
  package/database migration runs afterward as before.

### Changed

- **Web UI and CLI authentication moved from session cookies to per-device
  signing keys.** Logging in now enrolls an Ed25519 public key with the
  server, and every API request is signed with the matching key instead of
  carrying a session cookie. Enrolled keys are tracked and managed the way
  sessions were: System → Active Sessions shows each key's user agent and
  last-active time and can revoke it, and keys idle for 30 days are removed.
  All existing sessions are signed out on upgrade — sign in again on each
  device. Resolves #3511.

  The device key is a non-extractable WebCrypto key held in IndexedDB: page
  scripts can sign with it while the page is open, but can never read the key
  material out. Logging in requires a browser with Ed25519 WebCrypto support —
  any evergreen browser (Safari 17, Firefox 130, Chrome/Edge 137, or newer).

  HTTP cookies are gone from the API entirely: the server no longer sets or
  reads any cookie, so cookies planted by services co-hosted on other ports of
  the same hostname can no longer collide with StartOS auth. `start-cli` run on
  the server itself now presents the local authcookie as an
  `Authorization: Bearer` header instead of a `Cookie`, and its `--cookie-path`
  flag and on-disk cookie cache are removed.

  Each request signature is bound to a server identity — a hostname, domain,
  or IP address the server recognizes as itself — so a signature captured on
  one server can't be replayed to another. Those identities are the server's
  hostnames, its public and private domains, `localhost`, and its own
  interface addresses (including loopback and the public IP, for clients
  reaching it through a port forward). A signature that matches none of them
  is rejected with an error that says so, instead of the previous opaque
  "no valid signature context available to verify".

- **Changing the master password no longer asks for the current password.**
  **System → Change Password** in the UI and `start-cli auth reset-password`
  now prompt only for the new password — an authenticated session (an enrolled
  device key, or the local authcookie for `start-cli` on the server itself) is
  required and sufficient. This also gives "forgot password" a recovery path
  short of reinstalling: any device that is still signed in can set a new
  password. Existing backups remain encrypted with the password in effect when
  they were created.

- **Stable, predictable IPv6 address (EUI-64).** NetworkManager is set to derive
  each interface's IPv6 address from its MAC (modified EUI-64) with RFC 4941
  privacy extensions off. StartOS applies this to existing network connections
  on every boot, not just newly-created ones, so upgraded servers pick it up too.
  The server therefore keeps one stable global-unicast address (GUA) across
  reboots instead of the default rotating stable-privacy address, giving the
  GUA-based clearnet and public-domain features a predictable address to
  advertise (`AAAA`) and pinhole.
- **External ports 9050 and 9051 are no longer restricted (#3407).** The port
  allocator reserved 9050/9051 for the 0.3.x host Tor daemon, which no longer
  exists. Freeing 9050 lets the tor service bind its SOCKS proxy with
  `preferredExternalPort: 9050` (without exporting an interface), giving every
  service a stable, always-valid service-to-service address for Tor SOCKS on
  the internal bridge — `10.0.3.1:9050` — with no reactive watch on the tor
  package and therefore no dependent restarts when tor is installed, updated,
  or removed. 9051 (the old control port) is freed as well; 0.4.x tor uses a
  Unix control socket, so nothing binds it host-side.
- **The StartOS admin UI is now addressed like a regular service interface (#3387).**
  At the SDK/effects layer the server's own host is identified by the reserved
  package id `start-os`, host id `admin`, and interface id `admin-ui` (renamed
  from `startos-ui`) — no more `null`/`STARTOS` sentinels.
  `host_for`, the host RPC APIs, and the `getHostInfo` / `getServicePortForward` /
  `getServiceInterface` / `listServiceInterfaces` effects all resolve
  `start-os` to the server host, `start-os.startos` resolves like any package
  hostname, and the UI passes `start-os` wherever a package id is expected.
  Installing a package with the id `start-os` is rejected. The migration
  re-points the tor package's persisted hidden-service identity for the admin
  UI (`STARTOS`/`startos-ui` → `start-os`/`admin`) — including the onion-address
  import handoff written into tor's volume before tor is installed — preserving
  the server's existing `.onion` address across the identity change.
- **SDK:** `PluginHostnameInfo.packageId` is required in the type — url plugins
  (e.g. tor) should export the StartOS UI's urls as `start-os`/`admin` instead of
  `packageId: null`. For backwards compatibility during the beta.10 transition,
  the host still accepts the legacy `packageId: null` (or an absent field) over
  the effects socket and maps it to `start-os`.
- **OS Logs and Kernel Logs moved into System settings.** The top-level Logs tab
  is removed; OS Logs and Kernel Logs are now entries at the bottom of the System
  menu.
- **Migrated `startos-backup-fs` into the monorepo** as the `start-os/backup-fs`
  workspace member (from the former `Start9Labs/start-fs` repo); it is no longer
  built as an external `cargo install --git` dependency.
- **Monorepo reorganization.** `start-os` is now the monorepo for all Start9
  products. The OS product moved into its own `start-os/` directory as a thin
  wrapper: the `startbox` and `start-container` entry points live in
  `src/bin/`, the admin UI and setup wizard in `web/`, and the container runtime
  in `container-runtime/`. Backend logic moved from the old `core/` crate to the
  shared `start-core` crate (`shared-libs/crates/start-core`); shared Angular
  libraries moved to `shared-libs/ts-modules`; the SDK to `start-sdk`; and the `patch-db`
  submodule to `shared-libs/crates/patch-db`. Builds now run against the root Cargo and
  Angular workspaces (`cargo build -p start-os`, web from `shared-libs/ts-modules`).
- **Firewall migrated from iptables to native nftables (5b9cf7313).** Every StartOS-managed rule now lives in a single `table ip startos` with stable comment tags for handle-based, idempotent teardown (per-forward DNAT/hairpin/masquerade, the FORWARD `policy drop`, lxcbr0 container-egress accept, and the mangle policy-routing marks). lxc-net and wg-quick keep their own iptables-nft rules in separate tables. The `nftables` package is added to dependencies.
- **Manifest capability flags split (#3271, #3275).** The misleading `nestedRuntime` flag is replaced by two independent capabilities: `userspaceFilesystems` (mounts `/dev/fuse` for fuse-overlayfs storage) and `virtualNetworking` (mounts `/dev/net/tun` for VPN / WireGuard / tun workloads). Both are device grants only — the service LXC already retains `CAP_NET_ADMIN` within its user namespace via the standard `userns.conf` include, so no capability machinery is needed (an earlier `lxc.cap.drop` snippet that was wrongly framed as "re-granting `CAP_NET_ADMIN`" — and actually dropped five caps that were otherwise kept — was removed). Hard rename — packages using `nestedRuntime` must republish.
- **Service-container memory isolation (#3304).** Every service container is placed in a `services.slice` opted into systemd-oomd PSI monitoring, capped at total RAM minus a fixed 1 GiB host reservation; `system.slice`/`user.slice` get `MemoryMin` floors. A burst of concurrent installs can no longer overcommit RAM and wedge the host.
- **Container-runtime RPC/action logging is gated behind a dev build (#3325)** — production builds no longer log full RPC inputs/responses (which can contain action secrets) to service logs.
- **Web platform upgraded to Angular 22, TypeScript 6, and Taiga UI 5.11**, with a unified, version-pinned Prettier config enforced in CI.
- **SDK 2.0.0** ships alongside this release (see [`../start-sdk/CHANGELOG.md`](../start-sdk/CHANGELOG.md)); StartOS 0.4.0-beta.10 is its minimum host version.
- Backup progress is surfaced as a dialog with a percentage rather than a notification.
- **Dialogs and alerts no longer steal focus when they open.** `tuiAutoFocus` is
  gone from every surface that used it — the shared prompt dialog, the refresh
  alert, the OS-update dialog, the marketplace package drawer, and the setup
  wizard's password page. On mobile, autofocusing raised the keyboard the
  instant the sheet appeared, on top of the dialog's own buttons.
- **Docs prepared for general availability.** The _Update to StartOS 0.4.0_
  guide now documents the over-the-air update as the standard (and only
  documented) path — the USB-install walkthrough is removed — and the
  early-access warnings are gone. _Installing StartOS_ gains Raspberry Pi
  microSD flashing instructions (**Raspberry Pi 4 only**), which the update
  guide points Raspberry Pi users to, since a Raspberry Pi cannot update in
  place.
- **0.3.5.1 update prompt.** The release notes served with the 0.3.5.1 → 0.4.0
  migration OTA payload now carry the migration guide's full pre-update
  checklist instead of a one-line summary.
- **Release welcome.** The post-update welcome notification and its "What's
  new" highlights land with 0.4.0, so servers updating from any 0.4.0 beta see
  them on arrival at the stable release.
- **Service install backups are now constant-time btrfs snapshots.** The
  rollback backup taken before every service install/update used to be a
  file-by-file reflink copy whose cost grows with the volume's extent count —
  on a large, aged database volume (e.g. a 263 GB Monero LMDB fragmented into
  47 million extents) it pinned the update at ~80% for 19 minutes with no
  feedback. Package volume roots are now btrfs subvolumes, so the backup is an
  atomic `btrfs subvolume snapshot`: milliseconds regardless of size or
  fragmentation, and still zero data copied. Existing volumes are converted
  once, at the first boot after upgrading, under a new "Optimizing storage"
  boot phase with byte-accurate progress — expect that one boot to take
  longer on boxes with large service volumes. A volume root that still isn't
  a subvolume afterwards (e.g. on a non-btrfs dev data dir) simply skips the
  backup, as non-btrfs systems always have.

### Fixed

- **`start-cli --version` on StartOS reports the CLI version.** On the server,
  `start-cli` is a symlink to the `startbox` multi-call binary, which advertised
  the OS platform version for every applet — so `start-cli --version` printed the
  OS version instead of the independently-versioned start-cli version. It now
  reports the start-cli version, matching the standalone `start-cli` binary.
- **Backups wait for the service to fully stop before its data is copied.** A
  package's data is now captured only after its service has completely stopped,
  and the package isn't reported finished until it has left the backing-up
  state. Previously the backup could begin as soon as the package's `.s9pk`
  image finished serializing — often before the graceful shutdown had
  completed — so a slow-to-stop service could still be writing while its files
  were read, risking a torn or inconsistent snapshot of databases and other
  stateful data.
- **Migrating a 0.3.5.1 package that lacks instructions no longer fails.**
  Converting a legacy (Embassy) package to the new s9pk format now carries over
  its instructions when present and falls back to a placeholder when absent, so a
  package with no `instructions.md` can no longer break the 0.3.5.1 → 0.4.0
  upgrade. Previously the converted package omitted the file entirely.
- **Migrated 0.3.5.1 packages keep their asset file permissions.** Converting a
  legacy (Embassy) package to the new s9pk format now preserves the mode bits of
  files in the package's assets, so an executable asset (a script or bundled
  binary) stays executable in the migrated package. Previously the assets were
  unpacked without their permissions and re-packed at the default mode, dropping
  the executable bit and breaking any asset the service ran directly.
- **URL-plugin services no longer accumulate duplicate exported addresses.**
  `export_url` now dedupes a binding's `available` set by the same address
  identity `clear_urls` retains on (ignoring the row-action fields), so
  re-exporting a URL whose `remove_action`/`overflow_actions` changed updates
  the existing entry instead of inserting a second, `Ord`-distinct copy that
  `clear_urls` would also keep.
- **Each package's backup progress stays open until its image finishes
  writing.** A package's backup phase now completes only once the whole package
  backup — including streaming its `.s9pk` image to the backup target — is done,
  so the progress list keeps that package at 100% and still working until it
  truly finishes, then advances to the next one. Previously the phase was marked
  complete the moment the service's data procedure returned, while the image
  (often the larger part) was still writing to the target — which read as the
  package finishing early, with a visible pause before the next one began. Older
  packages built against start-sdk ≤ 2.0.6 still self-report completion and see
  the early "complete" until rebuilt against start-sdk ≥ 2.0.7.
- **The overall backup progress bar advances with each package's progress.**
  Every package now contributes equal weight (100) to the overall bar, with the
  OS-data step a small tail (10), so the top-level percentage climbs as each
  package's data copies rather than jumping only when a package finishes.
  Intra-package movement comes from the package's own reported progress, which
  packages built against start-sdk ≥ 2.0.7 report continuously.
- **Starting a backup returns as soon as the job is queued.** The "create
  backup" request now validates the backup password against the target and then
  hands the rest to the background job, so the initial call returns promptly.
  Previously it also opened the target's encrypted store on the request thread —
  which, for a large backup or a slow drive, could read for a minute or more
  before returning — so a slow target looked like the request itself had timed
  out. Opening the store, and any failure doing so, now happens in the
  background and is reported through the usual backup progress and notifications.
- **Backup progress shows an "Initializing" phase while the target is prepared.**
  Backup progress now displays a labeled "Initializing" phase while StartOS
  mounts the target and opens its encrypted store, instead of a bare 0% with no
  phases; the per-package and OS-data phases appear once that work finishes.
- **Mixed-case domains now match the browser.** Domain names are lowercased
  when added or removed (UI and CLI alike), so a domain entered with capital
  letters can no longer end up unreachable against the browser's lowercased
  address bar.
- **Login rate limiter no longer degrades logins to one per 20 seconds (#3512).**
  The password-login throttle used a single process-wide counter that only ever
  incremented and never reset, so after three logins since boot the entire box
  was capped at one login per 20 seconds across all clients (UI, CLI, API) for
  the rest of uptime. The counter now resets once 20 seconds pass without a
  further accepted login attempt — and a rejected attempt no longer advances the
  window — so the limit is a genuine three-attempts-per-20-seconds window rather
  than a permanent cap.
- **Large package installs no longer stall or freeze the box.** Downloading and
  unpacking a big s9pk streamed through the page cache with no writeback pacing,
  so on a fragmented copy-on-write btrfs filesystem the temporary and installed
  archives scattered across thousands of extents and their accumulated dirty
  pages flushed in one enormous final `fdatasync` — a stall lasting seconds to
  minutes whose victim varied by package size and each box's on-disk layout.
  Package transfers now reserve their space up front (`fallocate`), write to
  `nodatacow` files so they stay contiguous on btrfs, and pace writeback
  (`sync_file_range`) so only a bounded amount is ever left unsynced — keeping
  memory use flat and the final flush cheap.
- **DNS forwarder no longer wedges box-wide after an upstream blip (#3473).**
  After a WAN, tunnel, or DHCP event degraded the currently-configured upstream
  resolvers, container DNS could go dark for every service on the box — external
  lookups failing with `Temporary failure in name resolution` — until the
  upstreams recovered or the box was rebooted. The forwarder held a read lock on
  its upstream catalog across each upstream query (up to 30s) while the task
  installing new upstreams gave up after 10s and retried, starving the very
  update that would have replaced the dead upstreams. The resolver now snapshots
  the catalog and releases the lock before forwarding, and installs new upstreams
  by atomic swap — no lock wait, no timeout, no retry — so a pending upstream
  change always applies immediately. Forward queries also use a 5-second
  per-attempt upstream timeout (matching what container clients wait) rather than
  30 seconds, and names in the private `.startos`/`.embassy` zones that no
  running service claims are answered
  authoritatively (`NXDOMAIN`) instead of being forwarded to — and leaked at —
  upstream resolvers.
- **Enabling a public IPv4 address on an SSL service interface now opens the
  gateway port automatically.** Only a public _domain_ on an SSL-terminated port
  used to trigger automatic port forwarding (PCP/NAT-PMP/UPnP); turning on the
  bare public IPv4 left the port closed until it was forwarded by hand. StartOS
  now requests the pinhole for the SSL port the same way it already did for
  public domains and IPv6 GUAs, so a public IPv4 on an `addSsl` interface (the
  StartOS UI included) is reachable from the Internet without a manual forward.
- **Installer: "Preserve" selections that cannot keep your data are now refused instead of silently erasing the data drive.** During a USB install, choosing **Preserve** for a drive whose StartOS data pool lives on a _partition_ of the drive (the 0.3.x single-drive layout) while installing the OS to a _different_ drive fell through to creating a fresh, empty pool on the data drive — permanently destroying the data the user asked to keep, with no error. The installer now validates the preserve selection before writing anything and fails with an actionable error instead: a drive whose pool lives on a partition must be selected for **both** the OS drive and the data drive, and a drive whose pool spans the entire disk must be paired with a different OS drive. The setup wizard applies the same rules up front: when the selected drives cannot keep the data, the **StartOS Data Detected** dialog says why and disables **Preserve**, so an unpreservable selection is caught with guidance to fix it rather than surfacing as an error once the install is already under way.
- **IPv6 services exposed through a tunnel are now reachable, and outbound IPv6 no longer leaks around a gateway.** StartOS now applies the full IPv4 policy-routing layer to IPv6, including CONNMARK reply-routing: a reply to an inbound IPv6 connection that arrived over a tunnel — whether terminated on the host or DNAT'd to a service container — is pinned back out the interface it arrived on, so exposing a service over a StartTunnel's delegated IPv6 actually works. Previously those replies had no route back and were blackholed, so inbound IPv6 over a tunnel was dead. Outbound, the server's IPv6 default is now chosen by route metric exactly like IPv4, and leak prevention is per-gateway: an outbound gateway that is explicitly selected but can't carry IPv6 drops the server's IPv6 via a blackhole in that gateway's own routing table — so your real address never leaks out the ISP link — without blackholing the reply traffic that keeps inbound tunnel services working.
- **Updating a WireGuard gateway's config no longer drops its preshared key.** The in-place update path (`net tunnel update` / the **Update config** UI action, NetworkManager `Update2` + `Reapply`) persisted the interface private key but silently dropped each peer's preshared key, so a re-issued PSK-using tunnel failed its handshake and went dead (taking tunnel-routed DNS down with it). The peer secret is now flagged system-owned so the update persists it, and the settings (peer secrets inline) are passed to `Reapply` explicitly — an empty-dict `Reapply` still stripped the PSK from the _running_ device even with the profile persisted correctly, hanging all traffic through the tunnel (e.g. forwarded ports) until the next reboot.
- **Dev builds bricked by an empty persisted host id (#3387).** Builds between
  #3366 and #3387 persisted the server host's then-sentinel id (the empty
  string) in the admin UI interface's `addressInfo.hostId`, which strict
  deserialization rejects — every boot failed into the diagnostic UI. The
  server host now has a real id (`admin`) and the beta.10 migration rewrites
  the empty value.
- **Critical-task start gate is now enforced backend-side.** Starting a service with
  an unresolved critical task was previously blocked only in the web UI; the CLI and
  RPC bypassed it. `package.start` now rejects such a start unless `--force` is passed.
- **Split DNS for dual public/private domains (#3263).** A domain configured as both private (e.g. on Ethernet) and public (on a StartTunnel) is now served as private DNS to LAN clients, gated per-gateway in the resolver, instead of falling through to the upstream forwarder and hairpin-routing to the public VPS IP.
- **DNS `[::]:53` wildcard listener (#3346).** DNS listeners bind with `SO_REUSEPORT` so the dual-stack catch-all coexists with the per-address sockets; previously the catch-all's TCP bind failed with `EADDRINUSE` and was silently dropped.
- **Host address list renders instead of panicking (#3345)** — `start-cli` no longer hits `todo!()` displaying the server host address table.
- **Web server connections run in parallel with HTTP/2 adaptive window (#3328)**, fixing head-of-line stalls under load.
- **Logger writes moved off worker threads (#3259).** File/stderr log writes no longer hold a mutex across a blocking `stderr` write, which could park every tokio worker (and stall ports 80/443) if journald backpressured.
- **`create_task` self-deadlock during `setupInit` (#3273)** — a service calling `createOwnTask` with an `input-not-matches` trigger from its init handler no longer wedges in `updating`.
- **Replayed task state is preserved when the target service is unavailable (#3309)** — services running before shutdown no longer stay stopped after boot when a critical-severity task replays against a still-initializing dependency.
- **Stale mountpoints are reconciled before remount (#3314).** After a `SIGKILL` left kernel mounts in place, a same-boot restart now lazily unmounts the stale target instead of failing every service load with "already mounted".
- **`TMP_MOUNTS` self-deadlock on nested idmapped mounts** is avoided (5aee392a2).
- **Backups to a physical drive no longer intermittently fail with `could not load backup` (`NotFound`).** A temporary mount's teardown unmounted by path, so a stale detached unmount (e.g. from a `Drop`) could tear a drive down _after_ its shared `TMP_MOUNTS` slot had already been reused by a fresh mount at the same hashed path — and a concurrent operation loading `mount.backup-fs` off that drive then hit `ENOENT`. Teardown now runs under the per-mountpoint slot lock and unmounts only while the slot has no live guard, so it can't unmount a mount another operation just established. Surfaced by the periodic backup-target listing (backup page open) racing backup-fs loads on exFAT and BTRFS targets (#3498).
- **Unmountable partitions are skipped when listing backup targets (#3237)** instead of aborting the listing.
- **Bind-mount source directories are auto-created via recursive canonicalize** (d8ae7f199), and mount propagation is corrected (27322b4a9).
- **`/media/startos` ownership on migrated installs (#3311, #3312).** A migration repairs the stale `root:root` overlay entry so migrated nodes match fresh installs (`root:startos`), letting the `start9` SSH user browse package data without sudo.
- **CA fingerprint hex bytes are zero-padded**, with a `0.4.0-beta.10` repair migration for affected installs.
- **`.onion` (Tor) and `.local` (mDNS) are routed correctly through the host SOCKS proxy** (b5dec33cf), and reaching a `.onion` registry without Tor now reports a clear error ("the Tor service is not installed / not running") instead of a generic connection failure.
- **`NetService` is torn down synchronously on container destroy (#3285)**, and the nftables/policy-routing reconcile is made atomic, idempotent, and lock-free (daeee4f12, cebd6d703, 3ee50b0c3).
- **A service with no active bindings no longer wedges shutdown/restart indefinitely (#3350).** After #3285 moved network teardown into the awaited container-destroy path, a no-op `clear_bindings` emitted no patch-db revision, so the convergence wait blocked forever — stalling reboots for a service accessed only over Tor. The teardown now skips the wait when nothing changed.
- **Host `reboot`/`poweroff` now tears service containers down cleanly.** On an OS-level power-cycle, systemd stopped `lxc.service` (whose `ExecStop` is Debian's `lxc-containers stop`) and `lxc-monitord` _concurrently_ with the graceful-teardown hooks, killing each container before `startd`'s `Exit` RPC could stop it — a burst of `lxc-stop … is not running` / `Connection refused (os error 111)` and a stalled reboot. The teardown hooks (`startos-restart.service` / `startos-shutdown.service`) now order `After=lxc.service lxc-monitord.service`, so the LXC container-stop infrastructure stays up until the graceful teardown has finished. `Shutdown::execute` also skips its own `reboot`/`poweroff` when systemd is already performing the transition (and exits so systemd's stop of `startd.service` completes promptly). A power-cycle now tears containers down like `start-cli server restart`.
- **ACME issuance restored.** `async-acme` is re-pinned to keep its HTTP backend (#3342), and `ring` is installed as the process-default rustls provider so ACME cert acquisition no longer panics on the dual ring/aws-lc-rs build.
- **Union variant memory is isolated per row (#3337)**, disabled options render correctly in select dropdowns (#3229), and the per-row Wi-Fi overflow menu no longer disappears (#3243).
- **The redundant "Plugin:" prefix is dropped from interface plugin labels (#3349)** — the plugin address group already sits within a plugin section.
- **Login and CA-wizard UX cleanup (e86dcc242).** The login button uses an inline loading state that holds until navigation completes (replacing the global overlay loader) and is correctly centered; the CA wizard gets a solid card background, drops the redundant "Bookmark this page" step, and moves the "repeat on every device" caveat into a notification.
- **`fedimintd` → `fedimint-guardian`** package-ID rename is handled across all four `0.3.5.1`→`0.4.0` migration paths (2a806d8b8).
- **Allow non-ASCII characters in WiFi SSIDs (#3365).** Adding, connecting to, or
  removing a WiFi network whose SSID contains non-ASCII characters (e.g. an accented
  letter or a typographic apostrophe) no longer fails with "SSID may not have special
  characters". SSIDs are passed to NetworkManager as-is, and SSIDs containing a colon
  are now parsed correctly when listing connections. The WiFi passphrase still
  requires ASCII, as mandated by WPA.
- **Updates-tab progress circle vanishing during install finalization.** A
  package update marked the overall install progress complete right after the
  old version was uninstalled — before the new version's finalization/init
  progress (added with package init progress reporting, #3323) had run — so the
  Updates-tab loading circle snapped to full and then disappeared partway
  through "installing/finalizing". Overall progress now completes only once the
  package is fully installed, so the circle keeps tracking live finalization
  progress until the update finishes.
- **The UI no longer freezes after you enter your master password.** Creating a
  backup, restoring a backup, and changing your password each verified the
  password in the browser, via a _synchronous_ WASM Argon2id call on the main
  thread. At the parameters StartOS hashes with (64 MiB, t=3) that blocks the
  tab for seconds on a desktop and 20–25 seconds on a phone, where it reads as
  a hung browser; creating a backup onto a target whose existing backup used a
  different password ran it three times over. All three flows now let the server
  verify — which it already did on every one of these calls, natively, in
  milliseconds — and the argon2 WASM module is gone from the UI and
  setup-wizard entirely. A target whose existing backup was encrypted under a
  different password now comes back as the new `BackupPasswordMismatch` error
  kind, so the UI still knows to prompt for that original password rather than
  reporting the master password as wrong.
- **Backup password prompts no longer autocapitalize, and no longer look like a
  login to your password manager.** The shared prompt dialog sets
  `autocapitalize="off"`, so a mobile keyboard stops capitalizing the first
  character of a password you type into it, and it now submits through its own
  buttons (or Enter) instead of a `<form>` submit — a browser password manager
  saw that submit and offered to save your backup encryption password as a
  saved credential. The setup wizard's **Unlock Backup** prompt gets the same
  `autocapitalize` treatment.
- **A cancelled or failed package update now leaves the previous version
  running.** When an update is interrupted, StartOS restores the service's data to
  its pre-update state before restarting the old version, so the service comes back
  on the version it had. Previously the old version was started against the
  partially-migrated data and failed its downgrade migration with a "cannot
  migrate" error, leaving the service stuck until its container was rebuilt.
- **EFI system partitions are no longer listed as backup drives.** A GPT drive
  formatted on macOS or Windows (or a former boot drive) carries a small hidden
  EFI System Partition alongside its data partition, so the backup drive list
  showed the same physical drive twice — the second entry an unusable ~200 MB
  partition displayed as "0 GB". Partitions whose partition type marks them as
  an EFI System Partition are now skipped everywhere drives are listed (backup
  targets, setup-wizard drive lists, `start-cli disk list`), and a sub-gigabyte
  partition that does remain shows its capacity in MB instead of rounding down
  to "0 GB".
- **A transient failure querying clock-sync status no longer aborts boot or
  strands the clock-sync warning.** StartOS asks systemd (`timedatectl`) whether
  NTP has synchronized — once per second during the boot-time sync wait, then
  every 30 seconds in the background until the first sync lands. Both callers
  treated a failed query as fatal: during init it aborted boot into Diagnostic
  Mode, and in the background poller it silently killed the polling task,
  leaving the "Clock sync failure" warning up for the rest of the boot even
  after time synchronized. A failed query (e.g. a D-Bus activation timeout
  under boot load) is now treated as "not synchronized yet" — logged and
  retried on the existing cadence.
- **Install backups no longer silently fail for services using `nodatacow`.**
  btrfs refuses reflink clones between `nodatacow` (`chattr +C`) and normal
  files, so any service that marks its data directory `+C` — Bitcoin Core
  does — never actually got an install backup; the failure was only a log
  line. Subvolume snapshots are immune entirely, and the one-time boot
  conversion mirrors each file's `+C` flag before cloning it. Boot now also
  repairs interrupted backup restores (completing the rename so the backup's
  data comes back as the live volume) and sweeps backups orphaned by a
  package's removal; a backup left beside a healthy installed package is
  cleaned up on that package's next update.

### Removed

- **`/proxy` HTTP route.** The authenticated reverse-proxy endpoint was unused
  (registry data flows over RPC and package assets are served from local
  archives), so it has been removed.
- **Package `alerts` manifest field (BREAKING, #3333).** Packages can no longer define install / update / uninstall / restore / start / stop confirmation messages. StartOS stops reading and showing them; existing installs and old s9pks are unaffected (the field is ignored on load). Built-in confirmations for destructive actions are unchanged.
- **`nestedRuntime` manifest flag** — replaced by `userspaceFilesystems` / `virtualNetworking` (see _Changed_), with no compatibility alias.

### Security

- **The web UIs ship a strict Content-Security-Policy.** Every response from
  the UI origin — the main UI, setup wizard, and diagnostic/init pages — now
  carries a CSP restricting scripts and network connections to the server's
  own origin (no framing, no plugin content), plus
  `X-Content-Type-Options: nosniff`. A script injection that slips into the
  page can no longer pull in outside code or exfiltrate to a foreign host.
- **TSIG-authenticated DNS UPDATE (#3306).** RFC 2136 injections are authenticated with TSIG (RFC 8945, HMAC-SHA256) keyed off a per-device key derived (HKDF-SHA256) from that device's WireGuard PSK, closing a forgery vector where any co-located service emitting from the server's tunnel IP could inject DNS.
- **Packages are blocked from port-mapping the gateway (#3306).** Only startd may send UPnP/NAT-PMP/PCP upstream; a dedicated nftables guard table drops these protocols when forwarded from any interface (LXC), so a service can't open ports on the upstream gateway.
- **Packages are blocked from talking DNS straight to the gateway.** The same guard table now drops DNS (udp/tcp 53) forwarded off the container bridge, so a service can't query a gateway or public resolver directly — its DNS goes through the OS resolver at `10.0.3.1:53`, same as every other lookup.
- **Dependency/advisory cleanup:** resolved Dependabot alerts across core, web, and container-runtime (#3301); migrated to hickory 0.26 to clear DNS RUSTSEC advisories (#3302); resolved forked-dep RUSTSEC advisories in tokio-tar and async-acme (#3303).
- **Password hashes and backup key material are no longer sent to the
  frontend.** `serverInfo.passwordHash` — the master password's argon2 hash —
  was replicated into the public database, so every authenticated client held
  offline-cracking material for the master password. Separately, the backup
  metadata returned by `getBackupTargets`, `setup.cifs.verify`, and the disk
  listing carried each backup's `passwordHash` **and** its `wrappedKey`: the
  backup's encryption key sealed under that password, so a client could crack
  the hash offline and then unwrap the actual backup key. Both existed only to
  let the browser verify passwords locally, which it no longer does (see
  _Fixed_). `ServerInfo` drops the field entirely — the real hash stays in the
  private database — and the on-disk backup metadata is split from its public
  view, so the API returns only hostname, version, and timestamp. Existing
  servers are cleaned up automatically: startup deserializes the database
  through the typed model, which drops the removed key.

## [0.4.0-beta.9] and earlier

See the [GitHub releases page](https://github.com/Start9Labs/start-technologies/releases)
for the full 0.4.0 beta and alpha history and all prior releases.

[0.4.0]: https://github.com/Start9Labs/start-technologies/releases/tag/start-os/v0.4.0
[0.4.0-beta.9]: https://github.com/Start9Labs/start-technologies/releases/tag/v0.4.0-beta.9
