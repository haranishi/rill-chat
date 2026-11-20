# Implementation and verification

- [x] Implement authenticated workspaces, indexed history and targeted real-time delivery in Rust.
- [x] Implement the Japanese web client, fictional read-only preview and responsive navigation.
- [x] Provide an MIT license, contributor guide, development Compose, CI and a production promotion workflow.
- [x] Exercise channel and DM access, thread replies, reactions, search, duplicate-send handling, logout, reconnect and restart in independent sessions.
- [x] Measure loopback delivery with two sessions and inspect the actual UI at 390, 768 and 1440 pixels.
- [x] Configure the public repository, required checks on both branches, immutable version tags and owner-approved production environment. The release workflow verifies each distributable digest and publishes its receipt.

The Rust integration suite also checks stream invalidation, slow-consumer recovery and asset revalidation. The Chromium suite includes a disconnection lasting more than one history page and switching back from the phone layout. Reproduce the tests and performance measurements using the commands in CONTRIBUTING.md.

Local release verification on macOS arm64 (Rust 1.99.0, Chromium 159) measured POST-to-SSE p95 at 1.65 ms for 200 sequential messages and Enter-to-DOM p95 at 3.8 ms for 24 messages. These measurements use two independent authenticated sessions.

## Appearance revision

- [x] Apply light and dim palettes across the workspace, authentication, search and threads. Persist the appearance setting without storing account information.
- [x] Exercise keyboard, reload and theme selection in the browser. Verify contrast and review desktop and phone screenshots.
- [x] Open a separate development database over MCP, verify delivery and Japanese search with the Rust service, and record the supported deployment and multi-instance options.

Chromium at 390, 768 and 1440 CSS pixels showed the saved theme before the app script ran. Device changes and settings from a second tab updated the palette while preserving a draft. Independent review measured body text at 14.37:1 in light mode and 12.47:1 in dim mode. All 76 sampled text and control contrast pairs met their respective thresholds.

The SQLite development database passed a two-account Japanese send, SSE receive and search through Rust. DBHub 2.0.0 opened the same database over stdio MCP, confirmed the FTS match and served read-only queries with the committed configuration. The development file passed integrity and foreign-key checks after the test.
