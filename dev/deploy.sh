#!/bin/bash
# Builds the release and (re)starts it on the address .env names (BIND_ADDR, 127.0.0.1:3000), from
# the repository, in WSL: the one way to put the working tree on the development site, whoever
# does it. The server applies new migrations as it starts.
#
#   dev/deploy.sh             build, keep the running binary as the previous one, restart, check
#   dev/deploy.sh --rollback  restart the previous binary
#
# Before building it stops on a migration git doesn't know yet: the build would embed it and the
# server apply it, and a migration changed after being applied stops the server (its checksum).
# Commit it, or run with ALLOW_NEW_MIGRATIONS=1. A build that says it is done while the binary is
# older than the sources (a target directory another checkout shares, which cargo then trusts by
# the files' times) is redone from clean. If the new binary doesn't answer /healthz, the previous
# one goes back.
set -euo pipefail
cd "$(dirname "$0")/.."

TARGET="${CARGO_TARGET_DIR:-$HOME/.cache/marketplace-target}"
KEEP="$HOME/.cache/marketplace-deploy"
LOG="$HOME/.cache/marketplace-3000.log"
BINARY="$TARGET/release/marketplace"
ADDR="$(sed -n 's/^BIND_ADDR=//p' .env 2>/dev/null | tr -d '\r')"
ADDR="${ADDR:-127.0.0.1:3000}"
PORT="${ADDR##*:}"
mkdir -p "$KEEP"

# Puts the binary $1 in place as $2: a new file renamed over the old one, which a running server
# keeps executing (copying over it fails: "Text file busy").
put() { cp "$1" "$2.new" && mv -f "$2.new" "$2"; }

# The process listening on the port, if any.
listening() { ss -ltnp "sport = :$PORT" | grep -o 'pid=[0-9]*' | head -1 | cut -d= -f2; }

start() {
    local pid
    pid="$(listening || true)"
    if [ -n "$pid" ]; then
        kill "$pid"
        for _ in $(seq 50); do [ -z "$(listening || true)" ] && break; sleep 0.2; done
    fi
    setsid nohup "$1" >> "$LOG" 2>&1 < /dev/null &
    for _ in $(seq 100); do
        [ "$(curl -s -o /dev/null -w '%{http_code}' "http://$ADDR/healthz")" = 200 ] && return 0
        sleep 0.3
    done
    return 1
}

if [ "${1:-}" = "--rollback" ]; then
    [ -x "$KEEP/previous" ] || { echo "No previous binary in $KEEP." >&2; exit 1; }
    put "$KEEP/previous" "$KEEP/marketplace"
    start "$KEEP/marketplace" && echo "Rolled back: the previous binary is on http://$ADDR." && exit 0
    echo "The previous binary doesn't answer either; see $LOG." >&2
    exit 1
fi

new_migrations="$(git ls-files --others --exclude-standard migrations)"
if [ -n "$new_migrations" ] && [ "${ALLOW_NEW_MIGRATIONS:-}" != 1 ]; then
    echo "Migrations git doesn't know yet, which the server would apply:" >&2
    echo "$new_migrations" >&2
    echo "Commit them, or deploy with ALLOW_NEW_MIGRATIONS=1." >&2
    exit 1
fi

export CARGO_TARGET_DIR="$TARGET" SQLX_OFFLINE=true
cargo build --release
newest_source="$(find src templates migrations Cargo.toml Cargo.lock -type f -printf '%T@\n' | sort -n | tail -1)"
if [ "$(stat -c %Y "$BINARY")" -lt "${newest_source%.*}" ]; then
    echo "The binary is older than the sources: building again from clean."
    cargo clean -p marketplace --release
    cargo build --release
fi

# The running binary is the previous one now; the server runs from a copy, so a later build
# doesn't change the file under it.
[ -x "$KEEP/marketplace" ] && put "$KEEP/marketplace" "$KEEP/previous"
put "$BINARY" "$KEEP/marketplace"
if start "$KEEP/marketplace"; then
    echo "Deployed on http://$ADDR."
    exit 0
fi
echo "The new binary doesn't answer /healthz; the previous one goes back. See $LOG." >&2
if [ -x "$KEEP/previous" ]; then
    put "$KEEP/previous" "$KEEP/marketplace"
    start "$KEEP/marketplace" || true
fi
exit 1
