<p align="center">
  <img src="icon.svg" alt="{{name}} Logo" width="21%">
</p>

# {{name}} on StartOS

> Everything not listed in this document should behave the same as upstream {{name}}.
> If a feature, setting, or behavior is not mentioned here, the upstream
> documentation is accurate and fully applicable — see the Documentation section of
> `instructions.md` for links.

<!--
TODO: fill in the sections below, then delete these comments.

Read Writing Service READMEs first — it is the authority on who reads this file,
why the heading set is fixed and ordered, and what belongs in each section:

  ../src/writing-readmes.md
  https://docs.start9.com/packaging/writing-readmes.html
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
