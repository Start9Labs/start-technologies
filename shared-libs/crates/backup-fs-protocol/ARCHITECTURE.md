# Architecture

`FormatStatus` is an unauthenticated discovery hint. `ConversionProgress` is one newline-delimited JSON message emitted by the helper, with ordered phases and attempt-local counts. Log-record counts are bytes; object counts are steps. The core consumer translates these into its existing phased-progress tracker.

The helper owns inventory and completed work. Its authenticated migration marker and destination/pointer durability ordering own resumability; the protocol contains no recovery authority. A `Complete` event follows final superblock publication and discovery-marker cleanup.
