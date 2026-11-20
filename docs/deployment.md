# Running Rill Chat in production

One Linux host runs one container and one SQLite database. Releases include native images for x86-64 and ARM64. Any visitor can create a separate workspace. Membership in an existing workspace requires its owner's invitation.

## Prepare the host and the package

Set up Docker Engine, Bash, `flock`, `curl` and SSH on port 22. The deployment account needs a writable home directory and permission to use Docker. Dedicate this credential to the production host. The app listens on host loopback port 3847. This Caddyfile terminates HTTPS, preserves the original `Host` header and streams SSE responses.

```caddyfile
chat.example.org {
    reverse_proxy 127.0.0.1:3847 {
        flush_interval -1
    }
}
```

Replace the domain with your own and point it at this host. The production session cookie requires HTTPS. Allow enough disk space for the database and backup archives.

Enable GitHub Actions and the workflow's package, attestation and release permissions. To offer anonymous pulls, set the GHCR package visibility to public after its first publication. For a private package, configure a read-only registry credential for the deployment account on the host.

## Promote a release

Create the GitHub environment `production`. Restrict deployments to the `main` branch, select a required reviewer and configure the following environment settings.

| Setting | Value |
| --- | --- |
| Variable `PROD_HOST` | SSH hostname or IPv4 address |
| Variable `PROD_USER` | Deployment account on that host |
| Secret `PROD_SSH_KEY` | Deployment key paired with the public key installed on the host |
| Secret `PROD_KNOWN_HOSTS` | Full OpenSSH known-hosts line for `PROD_HOST`, verified through the host console or another trusted channel |

Run `gh workflow run deploy.yml --ref main -f version=v0.1.0` with a published version. GitHub verifies the source tag, its main ancestry and the release workflow's signed container digest. Read the verified digest in the first job's summary, then approve the pending environment deployment.

The host pulls the exact digest, stops the old container, archives its data, starts the new version on the same volume and probes `/healthz`. Clients briefly reconnect during replacement. A second deployment of the same healthy digest does nothing. Compose stores local messages in `rill-development_development-data`. Production uses `rill-production-data`.

## Roll back or recover data

For a compatible database schema, promote a previously published version using the same workflow. A failed cutover attempts to restart the previous container against the existing volume. If the new code changed the schema incompatibly, restore a pre-upgrade backup with its matching earlier version.

The rollout saves a cold archive in the `rill-production-backups` Docker volume before each upgrade. Set a retention policy and copy backups to encrypted off-host storage. To make an additional snapshot, run the following on the host while a deployment is idle. It restarts the app when archiving finishes.

```bash
(
  set -eu
  umask 077
  image=$(docker inspect --format '{{.Config.Image}}' rill-production)
  archive="rill-$(date -u +%Y%m%dT%H%M%SZ).tar.gz"
  docker stop rill-production
  trap 'docker start rill-production >/dev/null' EXIT
  docker run --rm --network none --read-only --user 0 \
    --mount type=volume,src=rill-production-data,dst=/data,readonly \
    --entrypoint tar "$image" -C /data -czf - . > "$archive.partial"
  tar -tzf "$archive.partial" >/dev/null
  mv "$archive.partial" "$archive"
)
```

Restore to a fresh host with an empty production data volume. Download the matching release's verified `rill-image.json` and copy the archive to this host. Extraction as root preserves the app's numeric file owner, UID 10001.

```bash
image=$(jq -er '.image + "@" + .digest' rill-image.json)
archive=/path/to/backup.tar.gz
docker volume create rill-production-data
docker run --rm -i --network none --read-only --user 0 \
  --mount type=volume,src=rill-production-data,dst=/restore \
  --entrypoint sh "$image" -ec \
  'test -z "$(ls -A /restore)" && tar -C /restore -xzf -' < "$archive"
```

Promote the matching release to the recovery host, sign in and check messages before switching traffic. Backups include private conversations and account data.
