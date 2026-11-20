# Rill workspace chat

A small team can open a browser, create a private workspace, invite a colleague, and carry on a conversation from another browser. A visitor can explore a clearly labeled, fictional studio workspace before registering.

1. Creating a workspace, joining by invitation and signing back in establish persistent, password-backed identities. Each account belongs to one workspace in v0.1.
2. Members can create channels, send and retrieve messages, exchange private direct messages, reply in threads, react, and search accessible history. Invites require the workspace owner. A signed-out browser cannot read real conversations.
3. Messages survive process restarts. Two simultaneous clients receive committed messages without polling. A disconnect, retry or channel switch must not duplicate messages or expose other conversations.
4. Keep the hot path small. Measure application send-to-receive latency on loopback with two sessions, report the sample size and p50/p95, and target p95 under 100 ms on a warm release build. This is a local acceptance target.
5. The Japanese interface works with a keyboard, Japanese IME and phone, tablet and desktop viewports. A user can recover from a failed send without losing the draft.
6. Publish original code under MIT. A contributor can run it locally and test a pull request. Integration work targets develop, main records stable changes, and a versioned image built from tested main is the input to production.
7. People who read chat for extended sessions can use a neutral light palette and an optional dim palette. Text, selection, reactions, keyboard focus and input errors remain distinguishable at desktop and phone sizes. Keep the identity of user-supplied content and swatches intact.
8. A developer can open a dedicated chat database over MCP, run the Rust service against that file and verify live delivery and Japanese search. Document the supported single-host architecture and a path to multi-instance delivery.
