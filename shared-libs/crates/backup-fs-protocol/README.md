# Backup filesystem protocol

Portable discovery and conversion-progress messages shared by `startos-backup-fs` and `start-core`. This crate has no FUSE or platform-specific dependency. The optional `reflection` feature exposes Rust wire metadata; `ts` adds the adjacent RPC binding bridges used by core's generators.

Discovery is advisory. Authenticated superblock state and the exclusive store lock authorize conversion and resumption. Progress counts never authorize skipping data or retiring source files.
