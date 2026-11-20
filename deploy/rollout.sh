#!/usr/bin/env bash
# Runs on a single production host, after the release receipt is verified.
set -euo pipefail
image=${1:?Expected a verified GHCR image digest}
[[ "$image" =~ ^ghcr\.io/[a-z0-9][a-z0-9._-]*/[a-z0-9][a-z0-9._-]*@sha256:[0-9a-f]{64}$ ]] || exit 2

app=rill-production
previous=rill-production-previous
volume=rill-production-data
backups=rill-production-backups
marker=io.rill.managed
exec 9>"$HOME/.rill-production.lock"
flock -w 120 9

exists() { docker container inspect "$1" >/dev/null 2>&1; }
managed() { [[ $(docker inspect --format '{{index .Config.Labels "io.rill.managed"}}' "$1") == production ]]; }
healthy() {
  [[ $(docker inspect --format '{{.State.Running}}' "$app") == true ]] &&
    curl --noproxy '*' --max-time 2 -fsS -o /dev/null http://127.0.0.1:3847/healthz 2>/dev/null
}
wait_healthy() {
  for _ in {1..20}; do
    if healthy; then return 0; fi
    sleep 1
  done
  return 1
}

# The fixed names make an interrupted cutover recoverable on the next run.
for name in "$app" "$previous"; do
  if exists "$name"; then
    managed "$name" ||
      { echo "Container name is already in use: $name" >&2; exit 1; }
  fi
done
recover() {
  if exists "$app"; then
    managed "$app" && docker rm -f "$app" >/dev/null || return 1
  fi
  if exists "$previous"; then
    managed "$previous" || return 1
    docker rename "$previous" "$app" || return 1
    docker start "$app" >/dev/null || return 1
    wait_healthy || return 1
  fi
}
if exists "$previous"; then
  if exists "$app" && healthy; then docker rm "$previous" >/dev/null; else recover; fi
fi
if exists "$app" && [[ $(docker inspect --format '{{.Config.Image}}' "$app") == "$image" ]] && healthy; then
  echo 'Requested digest is already healthy.'
  exit 0
fi

docker pull "$image"
docker volume create "$volume" >/dev/null
armed=false
on_exit() {
  result=$?
  trap - EXIT HUP INT TERM
  if [[ $armed == true && $result -ne 0 ]]; then
    echo 'Cutover did not finish. Recovering the previous container.' >&2
    if ! recover; then echo 'Recovery requires operator attention. The data volume is preserved.' >&2; fi
  fi
  exit "$result"
}
trap on_exit EXIT
trap 'exit 1' HUP INT TERM

if exists "$app"; then
  docker rename "$app" "$previous"
  armed=true
  docker stop -t 20 "$previous" >/dev/null
  snapshot="before-$(date -u +%Y%m%dT%H%M%SZ)-$$.tar.gz"
  docker run --rm --user 0 --read-only --network none \
    --mount "type=volume,src=$volume,dst=/source,readonly" --mount "type=volume,src=$backups,dst=/backup" \
    --entrypoint sh "$image" -ec 'umask 077; tar -C /source -czf "/backup/$1.partial" .; mv "/backup/$1.partial" "/backup/$1"' sh "$snapshot"
  echo "Saved $snapshot in $backups."
fi

armed=true
docker run --detach --name "$app" --restart unless-stopped --init \
  --label "$marker=production" --read-only --cap-drop ALL --security-opt no-new-privileges \
  --tmpfs /tmp:rw,nosuid,nodev,noexec,size=64m \
  --mount "type=volume,src=$volume,dst=/data" --publish 127.0.0.1:3847:3847 \
  --env RILL_ENV=production --env RILL_BIND=0.0.0.0:3847 --env RILL_DB=/data/rill.sqlite3 "$image"
wait_healthy
armed=false
if exists "$previous"; then docker rm "$previous" >/dev/null; fi
echo "Healthy: $image"
