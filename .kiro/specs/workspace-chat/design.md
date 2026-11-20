# Design

## User experience

| Layer | Decision |
| --- | --- |
| Strategy | Make the next message and its destination immediately apparent. |
| Scope | Channel chat, DMs, thread replies, a small reaction palette, search and invite-only workspace membership. |
| Structure | The sidebar picks a conversation. The timeline stays in the center. A contextual side panel displays details or the selected thread. |
| Skeleton | Keep the composer visible. Preserve a draft per conversation. On phones, open navigation and threads on demand to give the timeline the full screen. |
| Surface | Ink-green navigation, warm paper conversation area and a rust accent for actions. Names and text accompany color. Fictional preview data stays in the browser. |

The preview can be explored without signing up. All writes require a real account. A real workspace starts with empty channels; no example messages are copied into storage.

## Runtime

One Rust binary embeds the frontend assets. Axum/Tokio handles HTTP and Server-Sent Events (SSE). SQLite uses WAL, bounded history windows, conversation indexes and serialized writes. Event delivery occurs after commit. The initial SSE handshake and reconnects refresh authoritative state. Readers may fetch older messages using a cursor.

SQLite and one application instance are a deliberate fit for a self-hosted small team. The service stores password hashes and hashes of opaque session and invitation tokens. It checks workspace and conversation access on every operation and before sending events. Only the two participants can access a DM, including through search and streaming. The production profile requires secure cookies. Keep chats, accounts and invite codes out of public artifacts.

## Delivery

Feature changes go through pull requests into develop. A release pull request promotes develop to main. A semantic version identifies the exact main revision. Publish an immutable OCI image and deploy that digest to a separately configured production environment. Development and production use independent databases. See CONTRIBUTING.md and the operations guide for the actual commands.
