# Contributing to Rill Chat

Use a feature branch for a change. The Japanese browser interface and the Rust server ship together in one binary.

## Run a change locally

Install stable Rust and add the browser target with `rustup target add wasm32-unknown-unknown`. Start the service with `cargo run --locked` and open <http://127.0.0.1:3847>. Cargo builds and embeds the Rust browser crate automatically. Node.js 22 or later is used by the browser tests and benchmarks. The local server writes to `data/`. Docker Compose keeps its data in a separate development volume.

Start a branch from `develop` and open a pull request back to `develop`. Explain the change a user will see and how you tested it. Use fictional workspaces and messages in examples and issue reports.

```sh
cargo fmt --all -- --check
cargo clippy --locked -p rill-client -p rill-appearance --target wasm32-unknown-unknown -- -D warnings
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --workspace
cargo build --release --locked
npm ci --prefix tests
(cd tests && npx --no-install playwright install chromium)
node tests/browser.mjs target/release/rill-chat
node tests/bench.mjs target/release/rill-chat 200
```

The browser test and benchmark each start their own server and database. On a fresh Linux runner, install Chromium's system dependencies with `npx --no-install playwright install --with-deps chromium` from `tests/`. If you set `CARGO_TARGET_DIR`, pass the binary from that directory.

## From a pull request to a release

| Destination | How a change gets there |
| --- | --- |
| `develop` | Squash a feature pull request after the `Verify` and `Container` checks pass. |
| `main` | Open a release pull request from `develop`. Use a merge commit to keep the two branches related. |
| `vX.Y.Z` | Dispatch `Release` on the tested `main` commit. The workflow tags the source and publishes a multi-architecture image. |
| `production` | Dispatch `Deploy production` with a published version. The environment approver reviews the digest before rollout. |

Set the package version in a pull request and include the updated `Cargo.lock`. After CI has passed for the merged `main` revision, a maintainer can run `gh workflow run release.yml --ref main -f version=v0.1.0` with the chosen version. The release includes `rill-image.json`, which records its source commit and image digest. Configure the host and the GitHub environment before promoting that release. [Production operations](docs/deployment.md)

Maintainers should require pull requests and the two CI checks on both long-lived branches and block force pushes and deletions. Permit merge commits for promotions. Keep an open `develop` branch after promotion and protect published `v*` Git tags against edits and deletion.

Contributions are distributed under the MIT License. Third-party code and artwork need a compatible license and their required notices. The container includes Rust and crate notices under `/usr/share/doc/rill-chat/third-party/`. [Project license](LICENSE)
