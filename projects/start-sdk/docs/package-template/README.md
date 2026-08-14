<p align="center">
  <img src="icon.svg" alt="{{name}} Logo" width="21%">
</p>

# {{name}} on StartOS

> Everything not listed in this document should behave the same as upstream {{name}}.
> If a feature, setting, or behavior is not mentioned here, the upstream
> documentation is accurate and fully applicable — see the Documentation section of
> `instructions.md` for links.

<!--
TODO: Write this README per the packaging guide:
  ../src/writing-readmes.md

It documents how the StartOS package differs from upstream. Its readers are an AI
support agent, an AI assistant administering the server, and developers — end users
get instructions.md instead, and everyone who reads this file has that one too, so
never restate it.

The headings below are a fixed set, in this order: they are how an agent fetches part
of this file without loading all of it. Don't rename or reorder them. They run in four
groups — what the package is made of, how it behaves, what to expect when it doesn't,
then meta. Delete a section only where the guide says you may (Tasks and
Troubleshooting, if the package has neither). Open every section with a sentence or two
of prose before any table — that text becomes the section's summary in the generated
index.

Don't restate what StartOS already exposes (action ids, descriptions, allowed
statuses, input schemas): document when to run a thing, what it costs, whether it is
safe to repeat, and what it changes. No version numbers or image tags anywhere — the
manifest is the source of truth. Remove these comments when you're done.
-->

## Table of Contents

<!-- TODO: link every section present below -->

## Image and Container Runtime

<!-- TODO: image source, architectures, entrypoint, and the subcontainers this package runs -->

## Volume and Data Layout

<!-- TODO -->

## File Models

<!-- TODO: each config file the package owns — format, how it's seeded, what rewrites it, and whether a hand edit survives -->

## Dependencies

<!-- TODO: list each dependency, or "None." -->

## Network Access and Interfaces

<!-- TODO -->

## Installation and First-Run Flow

<!-- TODO: what setup differs from upstream, and any ordering the user must respect -->

## Actions

<!-- TODO: each user-facing action — when to run it, what it changes, cost, repeat safety. Or "None." -->

## Tasks

<!-- TODO: each task — what raises it, severity, what clears it. Delete the section if the package raises none. -->

## Health Checks

<!-- TODO: what each check probes, and what a failure means -->

## Backups and Restore

<!-- TODO -->

## Limitations and Differences

<!-- TODO -->

## Troubleshooting

<!-- TODO: symptom → check → action, for this package's real failure modes. Delete if there are none beyond upstream's. -->

---

## Quick Reference for AI Consumers

```yaml
package_id: '{{id}}'
image:
architectures: []
subcontainers: []
volumes: {}
file_models: []
startos_managed_env_vars: []
dependencies: []
interfaces: {}
actions: []
tasks: []
health_checks: []
```
