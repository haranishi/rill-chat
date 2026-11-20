# Implementation and verification

- [x] Implement authenticated workspaces, indexed history and targeted real-time delivery in Rust.
- [x] Implement the Japanese web client, fictional read-only preview and responsive navigation.
- [x] Provide an MIT license, contributor guide, development Compose, CI and a production promotion workflow.
- [x] Exercise channel and DM access, thread replies, reactions, search, duplicate-send handling, logout, reconnect and restart in independent sessions.
- [x] Measure loopback delivery with two sessions and inspect the actual UI at 390, 768 and 1440 pixels.
- [x] Configure the public repository, required checks on both branches, immutable version tags and owner-approved production environment. The release workflow verifies each distributable digest and publishes its receipt.

The Rust integration suite also checks stream invalidation, slow-consumer recovery and asset revalidation. The Chromium suite includes a disconnection lasting more than one history page and switching back from the phone layout. Reproduce the tests and performance measurements using the commands in CONTRIBUTING.md.

Local release verification on macOS arm64 (Rust 1.99.0, Chromium 159) measured POST-to-SSE p95 at 1.65 ms for 200 sequential messages and Enter-to-DOM p95 at 3.8 ms for 24 messages. These measurements use two independent authenticated sessions.
