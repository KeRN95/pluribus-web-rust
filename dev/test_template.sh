#!/bin/sh
# Makes the development database's template1 a migrated copy of the schema, so that every database
# the tests make (#[sqlx::test] runs `create database`, which copies template1) starts migrated:
# copying 12 MB of files takes a tenth of a second, where running the 26 migrations took half a
# second for each of the 118 tests. The migrator then finds every migration applied and does
# nothing. Run it after `docker compose up -d`, and again whenever migrations/ changes: a new
# migration only costs the tests its own run until then, but one changed under the tests fails
# them all ("migration N was previously applied but has been modified"). Scratch databases made
# with `sqlx database create` come migrated too, which is what they are for.
#
# Needs sqlx-cli (cargo install sqlx-cli --no-default-features --features postgres), and the
# database of compose.yaml on localhost:5432.
set -e
cd "$(dirname "$0")/.."
URL="${TEST_TEMPLATE_URL:-postgres://marketplace:marketplace@localhost:5432}"
command -v sqlx > /dev/null || { echo "sqlx-cli is needed: cargo install sqlx-cli --no-default-features --features postgres" >&2; exit 1; }
sql() { docker compose exec -T db psql -U marketplace -d "$1" -qAt -v ON_ERROR_STOP=1 -c "$2"; }

# template1 anew from template0, which stays as PostgreSQL made it.
sql postgres "ALTER DATABASE template1 IS_TEMPLATE false"
sql postgres "DROP DATABASE template1"
sql postgres "CREATE DATABASE template1 TEMPLATE template0"
sql postgres "ALTER DATABASE template1 IS_TEMPLATE true"
sqlx migrate run --database-url "$URL/template1" --source migrations
# The planner's statistics come along with the files.
sql template1 "ANALYZE"
echo "template1 is migrated to $(sql template1 "SELECT max(version) FROM _sqlx_migrations" | tr -d ' ') ($(sql postgres "SELECT pg_size_pretty(pg_database_size('template1'))" | tr -d ' '))."
