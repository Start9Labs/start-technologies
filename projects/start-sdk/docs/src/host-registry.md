# Hosting a Registry

A registry is the server that hosts, indexes, and distributes `.s9pk` packages and StartOS updates. Anyone can run one. This chapter covers running your own — from installing the packaged service on a StartOS device through day-to-day administration.

StartOS is built around an open registry model: no single entity controls what services are available, and packages can be distributed through any number of independent registries. Running your own makes you a distribution point in that ecosystem — useful for private testing, distributing to a specific audience (friends, customers, an organization), or maintaining packages indefinitely outside Start9's pipeline. Plenty of packages live this way permanently.

## What's in this chapter

- [Setup](host-registry-setup.md) — install the `startos-registry` service from the marketplace, walk through first-run setup (registry name, first admin, signing keys), and connect a local `start-cli` to the registry.
- [Administration](host-registry-administration.md) — day-to-day tasks: managing signers, publishing and removing packages, organizing categories, registering StartOS releases. Links out to the [start-cli registry reference](/start-os/cli-reference.html#registry) for command details.

## When you don't need to host your own

If you're publishing through the Start9 Community pipeline, you don't need your own registry to ship — that pipeline runs registries on your behalf. See [Publishing](publishing.md). The two paths aren't exclusive: developers often run a personal registry for alpha builds while a more stable version is promoted through Start9 Community.
