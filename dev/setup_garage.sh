#!/bin/sh
# One-time setup of the local photo store, after `docker compose up -d`: gives the single Garage
# node its storage, then creates the bucket and the access key the app uses (the same values as
# .env.example). Running it again does no harm.
set -eu
# Git Bash on Windows would turn /garage into a Windows path.
export MSYS_NO_PATHCONV=1

garage() { docker compose exec -T garage /garage "$@"; }

BUCKET=marketplace-media
KEY_ID=GKcdbe83a981b604008ecaaa58
KEY_SECRET=e53a24e44d0b706f7de2cabfc08e168d47ef3a4af989acdeb8d0eaf48834d379

# The node's storage: 10 GB in zone "local".
node=$(garage node id -q | cut -d@ -f1)
[ -n "$node" ] || { echo "Garage isn't running: docker compose up -d garage" >&2; exit 1; }
if garage layout show | grep -q "^$(echo "$node" | cut -c1-16)"; then
  echo "Garage layout already set up."
else
  garage layout assign -z local -c 10G "$node"
  garage layout apply --version 1
fi

garage bucket info "$BUCKET" >/dev/null 2>&1 || garage bucket create "$BUCKET"
garage key info "$KEY_ID" >/dev/null 2>&1 || garage key import --yes -n marketplace-app "$KEY_ID" "$KEY_SECRET"
garage bucket allow --read --write --owner "$BUCKET" --key "$KEY_ID"
echo "Garage is ready: bucket $BUCKET, key $KEY_ID."
