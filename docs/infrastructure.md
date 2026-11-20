# Choosing storage and live delivery

For a small self-hosted team, run one Rust service on a host with an attached local disk. Rill v0.2 saves each post in SQLite and then sends an event to the signed-in browsers allowed to read it. Clients fetch their history again on reconnect. The database, the event stream and a desktop client each solve a different problem.

| Deployment need | Choice | What it changes |
| --- | --- | --- |
| One host with persistent storage | SQLite with WAL, Axum and Server-Sent Events (SSE) | This is the configuration that ships. The database and Rust service share the host. The reverse proxy must stream responses promptly. |
| Two or more active Rust instances, or failover between hosts | PostgreSQL, for example on Supabase, with a shared event source | Each server needs to learn about commits on every other server. This requires a storage adapter and changes to delivery, search and session revocation. |
| Data reads close to users in other regions | Evaluate Turso Cloud with libSQL embedded replicas | Reads can use a local replica and writes go to the primary. Replication alone does not wake a browser connected to another Rust process. |
| A desktop client with OS integration | Tauri around the web interface | The desktop client would authenticate against the same service. Its packaging and native features can be evaluated independently of storage. |

## Operate one instance

Use a separate database for development and production. For production, the repository supplies a digest-pinned container rollout, a persistent volume, an HTTPS proxy example and a cold backup/restore procedure. Keep a recoverable copy off the host. SQLite WAL relies on shared memory on one machine. A network-mounted volume shared between application hosts is unsuitable. [Deployment and recovery](deployment.md) · [SQLite WAL](https://sqlite.org/wal.html)

Measure the path users actually experience. The repository's tests time a post until a receiving browser renders it. Also check disk commit latency and concurrent sends on the intended host. A second active instance or sustained contention after measuring on that host is a reason to revisit this choice. [Benchmark instructions](../README.md#performance-and-operations)

## A path to multiple servers

With PostgreSQL, store the message and call `pg_notify` within the same transaction. Send only an opaque event ID in the notification. Each Rust instance can then look up the event and apply existing workspace and DM access rules before sending SSE. Use a dedicated connection for `LISTEN`, either direct or through session pooling. Establish the subscription first, then catch up from committed rows. After a listener reconnects, repeat this sequence. [PostgreSQL NOTIFY](https://www.postgresql.org/docs/current/sql-notify.html) · [PostgreSQL LISTEN](https://www.postgresql.org/docs/current/sql-listen.html) · [Supabase connection modes](https://supabase.com/docs/guides/database/connecting-to-postgres)

Supabase also offers private Realtime Broadcast. Direct browser subscriptions would require mapping Rill identities to signed tokens and defining policies for channel and DM membership. Keeping SSE in the Rust service preserves the current browser protocol and session checks. Before promoting either design, exercise two Rust instances, third-party DM isolation, session revocation, reconnects, duplicate sends and Japanese substring search. [Supabase Realtime](https://supabase.com/docs/guides/realtime/subscribing-to-database-changes)

## Local database tooling

The development MCP setup opens a separate SQLite file. Its compatibility check exercises the FTS5 trigram index used for Japanese search. A trial of the local Turso Database 0.8.3 CLI's MCP could open a database, but it could not create this FTS5 table. That result applies to the local engine version tested. Hosted libSQL needs its own compatibility and latency tests. [Local database MCP](local-database-mcp.md) · [Turso embedded replicas](https://docs.turso.tech/features/embedded-replicas/introduction) · [Tauri overview](https://v2.tauri.app/start/)
