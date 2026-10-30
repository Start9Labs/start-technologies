# Backup filesystem protocol

This crate owns the shared helper discovery/progress wire types. Keep it portable: core also compiles for Darwin, while the filesystem implementation requires Linux. Authentication, filesystem mutation and task lifecycle belong to its consumers.

Run focused protocol checks with `cargo check -p backup-fs-protocol --all-features`. After wire changes, regenerate core bindings and verify both the helper producer and core consumer.
