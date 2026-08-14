# Security Policy

Start9's vulnerability disclosure policy is published at <https://start9.com/security>. That page is authoritative. This file says how it applies to this repository.

## Reporting a vulnerability

Email [security@start9.com](mailto:security@start9.com), or file privately on GitHub through the [draft advisory form](https://github.com/Start9Labs/start-technologies/security/advisories/new) — private vulnerability reporting is enabled on this repository. Use one of those two channels rather than a public issue, the forum, or a Matrix room.

Please give us a chance to fix the problem before you publish. We will not ask you to stay quiet indefinitely, and we will not ask you to sign anything. If you have a date in mind, say so in your report and we will work to it.

If you are unsure whether something is in bounds, ask at [security@start9.com](mailto:security@start9.com) first. We would rather answer the question than argue about it afterwards.

Include:

- The product and version you tested, and how it was installed (Start9 hardware, your own hardware, or a virtual machine). Quote the version exactly as the server reports it — StartOS versions may carry a fourth revision segment, as in `0.4.0.1`.
- The git hash, if you tested a build from `master`. Every `master` build of a product publishes under the same version number, so the hash is the only thing that identifies it. StartOS shows it in the web UI under the server's About dialog, next to the version.
- What an attacker gains, and what access they need to start.
- Steps to reproduce, ideally from a clean install.
- A proof of concept, if you have one. It saves everyone time. To keep an exploit out of plaintext mail, encrypt it to the key below, or use the GitHub advisory form.
- Whether you want credit in the release notes, and the name to use.

Machine-readable contact details are at <https://start9.com/.well-known/security.txt>, per RFC 9116.

### Our OpenPGP key

Encrypt your report to this key, or use it to check a Start9 signature:

```
Start9 <security@start9.com>
5456 DBFF 1B9D F905 041F  A776 5259 ADFC 2D63 C217
```

```
gpg --fetch-keys https://start9.com/start9.gpg
gpg --encrypt --armor --recipient security@start9.com report.txt
```

The key is also in this repository as [`apt/start9.gpg`](apt/start9.gpg) and on <https://keys.openpgp.org>. The same key signs the `stable` suite of Start9's Debian repository ([`apt/start9.list`](apt/start9.list)), so a machine that already installs Start9 packages carries a copy you can compare the fingerprint against.

## What happens next

Per the published policy, we will:

- Acknowledge your report within 5 business days.
- Tell you whether we consider it a vulnerability, and why.
- Keep you updated while we work on it, and tell you when the fix ships.
- Credit you in the release notes if you want the credit, and leave you out of them if you don't.

<https://start9.com/security> carries a safe harbour clause for research done in good faith and reported through this process; read it there for the terms themselves. Good faith means working only against your own devices and accounts, not accessing, modifying, or destroying anyone else's data, not degrading our services for other people, and stopping as soon as you have proved the point.

Fixes are described in the affected product's changelog under a `### Security` heading, for example [`projects/start-os/CHANGELOG.md`](projects/start-os/CHANGELOG.md).

## Scope

This repository holds six independently released products. A vulnerability in any of them belongs here:

| Directory                 | Product                                    |
| ------------------------- | ------------------------------------------ |
| `projects/start-os`       | StartOS, the server operating system       |
| `projects/start-wrt`      | StartWRT, the router OS                    |
| `projects/start-tunnel`   | StartTunnel                                |
| `projects/start-cli`      | `start-cli`                                |
| `projects/start-sdk`      | `@start9labs/start-sdk`, the packaging SDK |
| `projects/start-registry` | The registry server                        |

The shared code under [`shared-libs/`](shared-libs/) is part of those products and is in scope — `start-core` is the entire Rust backend. So are [`projects/start-docs`](projects/start-docs/) and [`projects/brochure-marketplace`](projects/brochure-marketplace/), which build Start9 websites, and the reusable CI in [`.github/workflows/`](.github/workflows/) that external service-package repositories call.

The registries Start9 operates and Start9's websites are in scope for the published policy. A flaw in a running service — a misconfiguration, an exposed endpoint, something you can only see from outside — goes to [security@start9.com](mailto:security@start9.com). A flaw in the code behind it belongs here: the registry server is [`projects/start-registry`](projects/start-registry/), and marketplace.start9.com and docs.start9.com are built from [`projects/brochure-marketplace`](projects/brochure-marketplace/) and [`projects/start-docs`](projects/start-docs/).

### Service packages live in their own repositories

Each service in the marketplace is packaged in its own `*-startos` repository, separate from this monorepo — Bitcoin Core is [`bitcoin-core-startos`](https://github.com/Start9Labs/bitcoin-core-startos), and the rest are listed under [Start9Labs](https://github.com/Start9Labs) and, for community packages, [Start9-Community](https://github.com/Start9-Community):

- **A flaw in the packaged application itself** goes to that application's upstream project. They can fix it and we cannot.
- **A flaw in the packaging** — the manifest, the health checks, the interfaces a package exposes, the credentials it generates — goes to [security@start9.com](mailto:security@start9.com) if an attacker could exploit it, or to the advisory form above with the package named. An issue opened on a `*-startos` repository is a public disclosure. Packaging bugs with no security impact are welcome there; if you are not sure which side of that line yours falls on, ask us first.
- **Tell us as well** at [security@start9.com](mailto:security@start9.com) if a Start9 packaging decision makes an upstream vulnerability worse, or if you think a package should be pulled from a registry.

The Community Registry distributes software written by other people, and the same split applies to it.

### Support questions are not vulnerability reports

A server that will not start, a service that will not connect, a lost password, or a backup that will not restore is a support question. Those get faster answers in the community channels listed on <https://start9.com/contact>. Use the security channels for a defect an attacker could exploit.

## Versions

Fixes land on `master` before they are released, and `master` is public — so a fix is readable in the commit history, and testable in the alpha builds of the products that publish there, ahead of the release that carries it. Merging and releasing are separate events, and the first one is public. Tell us if that timing matters to your disclosure plan.

Each product versions and tags independently as `<product>/v<version>`, and the git tags on this repository are the source of truth for what has shipped — a version number in a changelog heading or a manifest is the prospective next release, not a released one. Report against the latest release of the affected product where you can.
