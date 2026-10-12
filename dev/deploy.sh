#!/bin/bash
# Builds the release and (re)starts it on the address .env names (BIND_ADDR, 127.0.0.1:3000), from
# the repository, in WSL: the one way to put the working tree on the development site, whoever
# does it. The server applies new migrations as it starts.
#
#   dev/deploy.sh             build, keep the running binary as the previous one, restart, check
#   dev/deploy.sh --rollback  restart the previous binary, if it can run against the database
#
# Before building it stops on a migration git doesn't know yet: the build would embed it and the
# server apply it, and a migration changed after being applied stops the server (its checksum).
# Commit it, or run with ALLOW_NEW_MIGRATIONS=1. A build that says it is done while the binary is
# older than the sources (a target directory another checkout shares, which cargo then trusts by
# the files' times) is redone from clean. The new binary starts only once the old one has let go
# of the port (`port_taken`), and if it doesn't answer /healthz, the previous one goes back, the
# end of the log saying why: a deploy that goes back has failed, whatever /healthz says after it.
# Unless the previous one can't run against the database (`fits_database`), which has run a
# migration it lacks, the new one's: it would stop as it starts, leaving nothing on the port, so
# the script says so and leaves the port to the new one, whose log ends what it printed.
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

# Whether the port is still taken: a socket listening on it, whoever holds it. A server that is
# exiting keeps its socket until its last thread has gone, and `ss -p` stops naming it once its
# main thread has (its state "Zl"): waiting for the name alone, a deploy started the new binary
# into "Address already in use" and went back to the previous one, five times in four days, D2's
# on 2026-10-12 (D2b), the new jobs then never running on :3000.
port_taken() { ss -ltn "sport = :$PORT" | grep -q LISTEN; }

start() {
    local pid
    pid="$(listening || true)"
    if [ -n "$pid" ]; then
        kill "$pid" 2>/dev/null || true
    fi
    # Up to 20 s for the port to be let go of; then the old server is killed outright, and 5 s more.
    for _ in $(seq 100); do port_taken || break; sleep 0.2; done
    if port_taken && [ -n "$pid" ]; then
        kill -9 "$pid" 2>/dev/null || true
        for _ in $(seq 25); do port_taken || break; sleep 0.2; done
    fi
    if port_taken; then
        echo "Port $PORT is still taken; not starting $1." >&2
        return 1
    fi
    setsid nohup "$1" >> "$LOG" 2>&1 < /dev/null &
    for _ in $(seq 100); do
        [ "$(curl -s -o /dev/null -w '%{http_code}' "http://$ADDR/healthz")" = 200 ] && return 0
        sleep 0.3
    done
    return 1
}

# Whether the binary $1 can run against the database: whether it has the migration the database
# ran last, as the database ran it. One that lacks it stops as it starts ("migration 44 was
# previously applied but is missing"), so that going back to it leaves nothing on the port, as on
# 2026-10-09, when a new binary had migrated the database and then failed. sqlx keeps each
# migration's checksum in the binary beside its text, and the database keeps it in
# _sqlx_migrations: the database is asked through psql in the container that publishes
# DATABASE_URL's port (compose.yaml's db; WSL has no psql), and the binary searched for the
# checksum's bytes with python3. Where either can't be done, it is taken to fit, as it was before
# this check (C12). Says why when it doesn't fit.
fits_database() {
    local url pattern user port db container newest version checksum description found=0
    url="${DATABASE_URL:-$(sed -n 's/^DATABASE_URL=//p' .env 2>/dev/null | tr -d '\r')}"
    pattern='^postgres(ql)?://([^:@/]+)(:[^@/]*)?@[^:/?]+(:([0-9]+))?/([^?]+)'
    [[ "$url" =~ $pattern ]] || return 0
    user="${BASH_REMATCH[2]}" port="${BASH_REMATCH[5]:-5432}" db="${BASH_REMATCH[6]}"
    container="$(docker ps -q --filter "publish=$port" 2>/dev/null | head -n 1)" || return 0
    [ -n "$container" ] || return 0
    newest="$(docker exec "$container" psql -U "$user" -d "$db" -At -c \
        "SELECT version, encode(checksum, 'hex'), description FROM _sqlx_migrations ORDER BY version DESC LIMIT 1" 2>/dev/null)" ||
        return 0
    IFS='|' read -r version checksum description <<< "$newest"
    [ -n "$checksum" ] || return 0
    python3 -c 'import sys; sys.exit(0 if bytes.fromhex(sys.argv[2]) in open(sys.argv[1], "rb").read() else 3)' \
        "$1" "$checksum" 2>/dev/null || found=$?
    [ "$found" -eq 3 ] || return 0
    echo "$1 lacks migration $version ($description), which the database has run: it would stop as it starts (\"migration $version was previously applied but is missing\")." >&2
    return 1
}

if [ "${1:-}" = "--rollback" ]; then
    [ -x "$KEEP/previous" ] || { echo "No previous binary in $KEEP." >&2; exit 1; }
    fits_database "$KEEP/previous" || { echo "So it doesn't go back; the one running stays." >&2; exit 1; }
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
newest_source="$(find src crates templates migrations Cargo.toml Cargo.lock -type f -printf '%T@\n' | sort -n | tail -1)"
if [ "$(stat -c %Y "$BINARY")" -lt "${newest_source%.*}" ]; then
    echo "The binary is older than the sources: building again from clean."
    cargo clean --release -p marketplace -p pluribus-core -p pluribus-db -p pluribus-services -p pluribus-web -p pluribus-api
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
echo "The new binary doesn't answer /healthz. The end of $LOG:" >&2
tail -n 5 "$LOG" >&2
if [ -x "$KEEP/previous" ]; then
    # The previous one goes back only if it can run against the database, which the new one may
    # have migrated before it failed; otherwise the port is left to the new one, running still if
    # it is (a long migration, say), its log the last word rather than the previous one's refusal.
    if fits_database "$KEEP/previous"; then
        echo "The previous one goes back." >&2
        put "$KEEP/previous" "$KEEP/marketplace"
        start "$KEEP/marketplace" || { echo "It doesn't answer either. The end of $LOG:" >&2; tail -n 5 "$LOG" >&2; }
    else
        echo "So it doesn't go back: :$PORT is left to the new one, whose log ends above, to come up once it has started, or to be fixed and deployed again." >&2
    fi
fi
exit 1
