# Writing Service Instructions

`instructions.md` is a required file at the root of every StartOS package, alongside `README.md`. Its contents are packed into the s9pk archive and surfaced to the user under the **Instructions** tab on the service details page in StartOS.

Instructions are **for the human running the service** — not for developers, not for AI assistants. They pick up where the marketplace listing left off: by the time someone reads this tab they have seen the short and long description and clicked Install, so don't reintroduce the service. Orient them to what it does _on StartOS_, walk them through getting it usefully running, and point them at upstream documentation when they need to go deeper.

## Instructions vs. README — they are not the same file

It is tempting to treat `instructions.md` as a copy of the README. Resist this. The two files serve different audiences and answer different questions.

|                           | README                                                                                           | instructions.md                                                 |
| ------------------------- | ------------------------------------------------------------------------------------------------ | --------------------------------------------------------------- |
| **Audience**              | Developers, AI assistants, contributors                                                          | End users running the service on StartOS                        |
| **Question it answers**   | "How does this package work, and how does it differ from running the upstream service directly?" | "I just installed this — now what? How do I use it on StartOS?" |
| **Tone**                  | Technical, structured, scannable for parsing                                                     | Practical, instructional, written in second person              |
| **Versions / image tags** | Avoided (manifest is source of truth)                                                            | Avoided for the same reason                                     |
| **Upstream behavior**     | "Anything not listed here behaves as upstream documents"                                         | Linked from the Documentation section; never duplicated         |
| **Surfaced where**        | The package repository on GitHub                                                                 | Inside the StartOS UI, post-install                             |

If your README is a reference manual, your instructions are a quick-start guide for a non-developer who just clicked Install.

## What belongs in instructions

A good `instructions.md` covers, roughly in this order:

1. **A brief orientation — usually skip it.** The reader already saw the marketplace short and long description before clicking Install, so don't restate them. The default is to omit this section and go straight to **Documentation**. Add a line only if there is genuinely new context the listing did not cover — a hard ordering constraint, a permanent decision the user is about to make, or similar. "You've installed X" framing is _not_ useful; the reader knows. Don't pad.

2. **Documentation links.** A `## Documentation` section. Port exactly the URLs the manifest previously carried in its `docsUrls` array, each with a few words on what it is ("the upstream admin guide", "the official Foo configuration reference"). Do not add marketing, donation, project-home, or support-channel links — those live elsewhere and were deliberately omitted from `docsUrls`. Link to canonical, stable URLs the upstream maintains — not specific commits, not your own README.

3. **What it gives you on StartOS** — the practical answer to "why did I just install this?" Keep it concrete: the interfaces it exposes, the data it manages, the experience the StartOS package adds on top of upstream.

4. **Getting set up** — the smallest sequence of steps that takes a fresh install to a usable state. The reader has already installed the service — don't include download or install steps. Start from first launch. Use numbered lists. Reference real action names, real interfaces, and real screens that exist in the StartOS UI for this service. If setup requires a dependency, say so plainly: "Install Bitcoin Core first" rather than "satisfy the dependency."

5. **Using the available features** — once the service is running, what can the user actually do with it? Describe the interfaces (web UI, RPC, etc.) and the **user-visible** actions. Hidden actions (`visibility: 'hidden'` in the package source — typically those invoked by the platform or by another service rather than by a human) do not belong here; the user never sees them. Likewise, do not parrot `allowedStatuses` from action source code ("the service must be running", "the service must be stopped"): describe what the user actually encounters in the UI, and omit the qualifier when it's noise.

6. **Important limitations — usually omit.** The default is no Limitations section at all. Add one only if there is a specific, consequential thing the user will be surprised by: a deliberately disabled feature they may go looking for, a hard data caveat, an incompatibility worth flagging up front. Generic caveats ("performance depends on your hardware", "encryption keys are sensitive") are not limitations and do not belong here.

> [!NOTE]
> Older StartOS manifests carried a `docsUrls` array for upstream documentation links. That field has been removed — those links belong in the `## Documentation` section of this file now, where you can give each one the context a bare URL in the manifest never had.

## What does not belong in instructions

- **A restatement of the marketplace description.** The reader saw the short and long description before installing — opening with "Foo is a self-hosted bar" wastes their time. Start from "now what."
- **"You've installed X" or any other orientation that tells the reader something they already know.** They installed it; that's why they're on this tab.
- **Install or download steps.** They've already installed the service. Begin at first launch.
- **How StartOS itself works.** The interface panel's copy-address / QR-code / LAN-Tor-domain controls, the Dashboard and Instructions tabs, how backups and updates work, how to start or stop a service — these are platform features a user learns once, not per-package. Mention only what's specific to _this_ service: which interfaces it exposes and what each is for, which actions it adds and when to run them. Naming a screen to send the user to ("open it from the **Dashboard** tab") is fine; explaining what that screen is, isn't.
- **Invented navigation paths.** Don't guess at how to reach a UI surface. Reference only screens, tabs, and tables that actually exist in StartOS for _this_ service. "Set X in the network settings" is wrong if there is no such page; "add the domain on the Homeserver interface" is right if that's where it actually lives.
- **Hidden actions.** Actions marked `visibility: 'hidden'` in source — typically those invoked by the platform or by another package's plugin handshake — are not user-facing. Do not list them, even to "explain" them.
- **Status preconditions for critical tasks.** A critical task suspends every other control: the user does not see a Start / Stop / Run button while the task is required, only the task. Telling them "the service must be stopped" or "start the service first" before running a critical task is not just noise, it's wrong.
- **Platform plumbing the user can't act on.** "Registration is typically triggered automatically by the bridge service" tells the reader nothing they can do with the information. If they'd never act on a sentence, cut it.
- **The full configuration reference.** Link to upstream for that.
- **Version numbers and image tags.** They go stale every release; the manifest is the source of truth.
- **Architectural detail about how the package is built.** That is the README's job.
- **Reasons the package was structured a particular way.** Users do not care.
- **Internal terminology from the StartOS codebase** ("ABI", "task", "manifest", "subcontainer"). Use the words a user sees in the UI.
- **Secrets, default passwords, or API keys hard-coded into the markdown.** Generate those at install time and surface them via actions.

## Style

- Write in the second person. "You will see…", "When you click…", "Before you start, make sure…".
- Prefer numbered lists for any multi-step procedure.
- Use code blocks for commands the user might run, hostnames they might paste, or RPC calls — not for prose.
- Keep paragraphs short. Many users will scan, not read.
- Use H2 (`##`) for top-level sections; reserve H1 for the service name at the top of the file.
- StartOS will render the markdown through the same pipeline as release notes and licenses, so standard CommonMark + GFM tables work; exotic HTML may not.

## Suggested structure

Use the sections that apply — a trivial service might be two paragraphs and a Documentation list; a complex one might need every section below and more. Don't include a section just to have it (if the service has no actions, you usually needn't say so).

```markdown
# [Service Name]

[Optional, usually omit. Add one or two sentences only if there is genuinely new context the marketplace listing didn't cover — for example, a hard ordering constraint or a permanent decision the user is about to make. Otherwise delete this line and start with the section below.]

## Documentation

- [Upstream documentation](https://docs.example.org) — what it is in a few words (the config reference, the upstream README, etc.).

(Port exactly the URLs the manifest previously carried in `docsUrls`. Don't add marketing, project-home, donation, or support links here.)

## What you get on StartOS

[Concrete description of the StartOS experience: which interfaces are exposed, what data it manages, what the package adds on top of upstream.]

## Getting set up

1. [First concrete step the user should take after install.]
2. [Second step…]
3. [Until the service is in a usable state.]

> If your service depends on another, list the dependency explicitly here and tell the user to install it first.

## Using [Service Name]

[Describe the day-to-day experience. Interfaces, actions, common workflows. One short subsection per major capability is fine.]

### Web interface

[What this interface is for and what the user sees first — a login screen, a setup wizard, an empty dashboard. Not how the universal interface-panel controls work; those are identical in every service.]

### Actions

[Each StartOS action: what it does, when to run it.]

### [Other capability]

[…]

## Limitations

[Usually omit this section entirely. Include only if there is a specific, consequential surprise — a deliberately disabled feature the user may go looking for, an incompatibility worth flagging.]
```

## Pre-publish checklist

- [ ] File exists at `instructions.md` at the package root (the build will fail otherwise).
- [ ] Written for the user, not the developer — no internal SDK terminology.
- [ ] Does not restate the marketplace short/long description, contains no install or download steps, no "You've installed X" framing, and doesn't explain StartOS platform features (interface controls, tabs, backups) the user already knows.
- [ ] All navigation references point at UI surfaces that actually exist for this service — no invented network-settings pages or pretend tabs.
- [ ] Setup steps walk from first launch to a usable state.
- [ ] Every action and interface mentioned actually exists in the package, and every action mentioned is `visibility: 'enabled'` (hidden actions are not listed).
- [ ] Status preconditions are described as the user actually sees them — critical tasks are not qualified with "stop the service first"; `allowedStatuses` is not parroted when its gate is invisible to the user.
- [ ] Every sentence is something the user could act on — no "this is typically triggered automatically by …" plumbing notes.
- [ ] No hard-coded version numbers, image tags, or secrets.
- [ ] Limitations section is omitted unless there is a specific, consequential surprise to flag.
- [ ] A `## Documentation` section ports the URLs the manifest's `docsUrls` previously carried, each with a few words of context. No added marketing / donation / project-home / support links.
- [ ] Renders cleanly in the StartOS Instructions tab on a real install.
