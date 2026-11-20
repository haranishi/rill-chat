# Rill Chat

Rill is a self-hosted team chat built in Rust, from message storage to the interactions in your browser. Create a workspace, invite a teammate and talk in channels, direct messages or threads. One executable includes the Japanese WebAssembly client.

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/preview-dim.png">
  <img alt="Rill’s fictional studio workspace, with a channel conversation and a member list" src="docs/preview.png">
</picture>

## Start chatting

On Linux, download the archive for your CPU from [Releases](https://github.com/haranishi/rill-chat/releases), check its SHA-256 against `SHA256SUMS`, extract it and run `./rill-chat` inside the extracted directory. The binaries target Debian 12 or a compatible glibc system.

To build from source with stable Rust, clone the repository and run the server.

```sh
git clone https://github.com/haranishi/rill-chat.git
cd rill-chat
rustup target add wasm32-unknown-unknown
cargo run --release --locked
```

Open [Rill on localhost](http://127.0.0.1:3847). The first screen lets you explore a read-only sample. Choose **チームで使う** to create a workspace. On a phone, open the menu to find this button. Existing members can sign in with **ログイン**.

Choose **メンバーを招待** to generate a 24-hour invitation code. A teammate opens the same server address, chooses **招待で参加**, and registers with that code. For a local trial, open a private browsing window as the second person. For access from other devices, run the server behind HTTPS as described in the [operations guide](docs/deployment.md).

You can also start the development server with Docker Compose.

```sh
docker compose up --build
```

## What the team can do

- Create channels, reply in threads, add reactions and chat privately with another member.
- Correct your own posts and replies. Colleagues see the revision live, and Japanese search finds the updated text.
- Choose **あとで読む** on a message to save it privately. Open its entry in the sidebar to return to the exact conversation or reply.
- Change your password from your account menu. Other signed-in devices must authenticate again.
- Search accessible messages, including Japanese text, and jump to the matching conversation.
- Get new messages as soon as they are committed. Reconnecting reloads stored history, and retrying a send safely reuses its message identifier.
- Use the appearance control in the top bar to choose **ライト**, **ダーク** or the device setting. The browser remembers the choice across visits.

The service uses Axum, Tokio and SQLite in WAL mode. Each account belongs to one workspace. Anyone with access to the service can create a separate workspace. The workspace owner issues invitations to join their team. The sample conversations stay in the browser.

## Performance and operations

The delivery benchmark starts an isolated server and measures from the start of a sender’s HTTP request until a second signed-in connection decodes the matching SSE event. The browser suite also measures from Enter in one Chromium session to the message appearing in another session’s DOM.

```sh
cargo build --release --locked
node tests/bench.mjs target/release/rill-chat 200
node tests/fanout.mjs target/release/rill-chat
```

Measured on a warm macOS arm64 loopback installation with SQLite durable commits enabled. Each backend figure is the median of three 200-post runs, alternating the two release binaries on the same Mac. The fan-out test delivered every message to all sixteen readers (3,200 deliveries per run).

| Measured path | v0.2 p50 / p95 | v0.3 p50 / p95 |
| --- | --- | --- |
| Sender HTTP request to peer SSE event | 0.43 / 1.08 ms | 0.44 / 0.84 ms |
| Four senders to the last of sixteen subscribers | 1.36 / 2.11 ms | 1.53 / 2.43 ms |

Pressing Enter in one v0.3 window and inserting the message in the other window's conversation took 1.6 ms at p50 and 1.8 ms at p95 in Chromium 159. In desktop WebKit 27.2 both values measured 4 ms at that browser timer's precision. Each browser ran 24 measured sends after four warmups. Network distance, storage and concurrency affect deployment results.

Deploy one application instance per SQLite database. Development Compose and production use separate volumes. The production workflow verifies a versioned image digest before the host starts it. [Infrastructure choices](docs/infrastructure.md) · [Connect a local development database over MCP](docs/local-database-mcp.md) · [Deployment and recovery](docs/deployment.md)

## Contribute and license

Open feature pull requests against `develop`. A release pull request promotes a tested change to `main`. See [Contributing](CONTRIBUTING.md) for the test commands and release procedure.

Read how Rill chooses features, keeps direct messages private and handles old databases. [Design principles and roadmap](docs/design-principles.md) · [Authentication design](docs/authentication.md) · [Software design document (SDD)](.kiro/specs/workspace-chat/design.md) · [API contract](.kiro/specs/workspace-chat/protocol.md)

Rill’s original code and artwork are available under the [MIT License](LICENSE).
