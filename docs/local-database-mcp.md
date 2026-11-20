# Inspect the development database with MCP

Use this connection to check what Rill stored during local development. It opens only `data/mcp-dev/rill.sqlite3` in this checkout. Rill handles migrations, message writes and notifications. DBHub gives an MCP client read-only SQL and schema inspection over standard input and output.

## Create the database

From the repository root, start a separate local Rill instance. Port `3848` lets the development instance run alongside the default server.

```sh
umask 077
mkdir -p data/mcp-dev
RILL_ENV=development RILL_BIND=127.0.0.1:3848 \
  RILL_DB="$PWD/data/mcp-dev/rill.sqlite3" cargo run --release --locked
```

Open `http://localhost:3848` in your browser and use `http://127.0.0.1:3847` for the default server. Browsers scope session cookies by host and can share them across ports, so these distinct hostnames let you sign in to both instances independently. Rill creates the database and enables WAL and its FTS5 trigram search index on first start. To try live delivery, open a second browser profile and join the first account's workspace with an invitation.

## Connect an MCP client

Use Node.js 24 and npm. From the repository root, use this command as the client's stdio server. The first launch downloads DBHub. The connection uses the SQLite driver included with Node.js. [DBHub configuration](https://github.com/bytebase/dbhub/blob/v2.0.0/docs/config/toml.mdx)

```sh
npx --yes --omit=optional @bytebase/dbhub@2.0.0 \
  --transport stdio --config deploy/dbhub.dev.toml
```

In a desktop client, use `npx` as the command and enter each argument separately. Pass the absolute path to the configuration file after `--config` so the client can start from any directory. DBHub resolves the database path relative to the [development configuration](../deploy/dbhub.dev.toml).

Call `tools/list` to see `search_objects` and `execute_sql`. For a quick check, call `execute_sql` with the following SQL. A new Rill database reports schema version `1` and journal mode `wal`.

```sql
SELECT user_version, journal_mode
FROM pragma_user_version, pragma_journal_mode;
```
