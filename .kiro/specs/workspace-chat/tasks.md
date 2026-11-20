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

## Rust client and message tools

- [x] Migrate existing workspaces and implement author-only edits, search reindexing and private saved items in channels, DMs and threads.
- [x] Port browser behavior to Rust/WebAssembly, expose edit, saved-item and account actions and keep the published flows usable.
- [x] Implement password change with session rotation, record the auth boundaries and publish the OSS feature-selection and compatibility policy.
- [x] Verify fresh and upgraded databases, cross-session privacy, reconnect, both themes, desktop/mobile flows, package reproducibility and measured release latency.

Two release binaries exercised the same disposable database across the v0.2 → v0.3 migration. Existing sessions, IDs, a DM thread and the original retry key survived. Integration tests exercise migration rollback, concurrent edits, Japanese reindexing, two-reader privacy, save pagination and live session revocation.

The browser suite passed on Chromium 159 and desktop WebKit 27.2. It verifies the Wasm loads, the stored palette paints while the larger module is delayed, and failed loads display recovery instructions. It exercises edits, stale drafts, private saves, thread/DM jumps and password changes at desktop and phone widths. The delayed-cookie test preserves drafts in a sibling tab and revokes an independent session. See the [published measurements](../../../README.md#performance-and-operations) for browser and backend delivery times.

Two clean offline release builds from different source and target directories on one machine served identical bytes for all seven static assets. The three precompressed responses decompressed to the corresponding originals and their ETags matched each representation. The Docker build context contains both Rust browser crates and their loader.
