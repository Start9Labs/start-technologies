# Packaging Guide

StartOS is a _Server OS_ -- a Linux distribution optimized for administering servers. While operating systems like Mac, Windows, and Ubuntu are designed for client devices such as phones and laptops, StartOS provides a graphical interface for server administration that eliminates the need to "pop the hood" and use the command line.

Through the StartOS web interface, users can discover, download, install, configure, monitor, back up, and generally manage any variety of self-hosted, open-source software.

## Designed for AI-Assisted Development

StartOS service packaging is designed to be done with an AI coding agent. This guide, the SDK, and every existing package are structured so that an AI assistant can read the docs, study real packages, and write or modify package code with minimal human intervention. You do not need to be an expert TypeScript developer -- you need to understand _what_ your service requires and let the AI handle _how_ to implement it.

The recommended setup is [Claude Code](https://claude.com/claude-code) with this guide and your package in the same workspace, scaffolded by `start-cli s9pk init-workspace`. See [Environment Setup](./environment-setup.md#set-up-your-packaging-workspace) and the [Quick Start](./quick-start.md) to get going.

## What is a StartOS Package?

What makes this experience possible is a unique package format (`.s9pk`) that permits services to take advantage of StartOS APIs. In its most basic form, a package is a thin metadata wrapper around a service that allows it to be discovered, installed, and run on StartOS. Beyond that, the StartOS APIs grant developers an incredible degree of creative capacity to define the end-user experience for their service. Developers can:

- Display instructions and tooltips
- Present warnings under certain conditions
- Run arbitrary code on install, update, and uninstall
- Represent configuration files as validated forms with all varieties of form inputs
- Define scripts and commands that present as buttons with optional inputs
- Write health checks that run on an interval and are optionally displayed
- Automatically install and configure dependencies
- Maintain state and optionally expose particular values to users or dependent services
- Grant users flexible networking options such as LAN, Tor, and clearnet
- Offer one-click, encrypted backups of targeted data

## Where to Start

1. **Set up your environment** — Follow [Environment Setup](./environment-setup.md), including the Claude Code section.
2. **Build your first package** — Follow [Quick Start](./quick-start.md) to create, build, and install the Hello World template.
3. **Use recipes to build your service** — Browse [Recipes](./recipes.md) to find the patterns you need. Each recipe describes _what_ to do, links to reference pages for API details, and points to real packages for working code. Your AI agent reads these docs and writes the code.

## Recipes

Intent-driven guides for common packaging tasks. These are the primary entry point for both you and your AI coding agent.

- [What Do You Want To Do?](./recipes.md) - Browse all recipes by intent

## Getting Started

1. [Environment Setup](./environment-setup.md) - Install the required development tools
1. [Quick Start](./quick-start.md) - Create, build, and install your first package
1. [Development Workflow](./workflow.md) - How to behave while working on a package

## Reference

1. [Project Structure](./project-structure.md) - Understand the file layout of a StartOS package
1. [Manifest](./manifest.md) - Define your service metadata and release notes
1. [Versions](./versions.md) - Handle install, update, and downgrade logic
1. [Main](./main.md) - Configure daemons, health checks, and the service lifecycle
1. [Initialization](./init.md) - Run code when your service initializes
1. [Interfaces](./interfaces.md) - Expose network interfaces to users
1. [Actions](./actions.md) - Define user-facing buttons and scripts
1. [Tasks](./tasks.md) - Prompt users to run actions at the right time
1. [File Models](./file-models.md) - Represent and validate configuration files
1. [Dependencies](./dependencies.md) - Declare and configure service dependencies
1. [Makefile](./makefile.md) - Automate build and install workflows
1. [Writing READMEs](./writing-readmes.md) - Write effective service documentation
