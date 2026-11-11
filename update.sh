#!/bin/bash
set -euo pipefail

REMOTE="https://github.com/rocketman510/Robo-Pope.git"

echo "[$(date -u +%Y-%m-%dT%H:%M:%SZ)] Starting deploy..."

# Fetch latest tags and commits
git fetch --tags --force "$REMOTE"

# Discard any local changes
git reset --hard

# Get the latest tag by version name
latest=$(git tag --sort=-version:refname | head -n1)

if [[ -z "$latest" ]]; then
  echo "ERROR: No tags found. Aborting." >&2
  exit 1
fi

echo "Deploying tag: $latest"
git checkout "$latest"

cargo build --release

pm2 startOrRestart ecosystem.config.js || pm2 restart index

echo "[$(date -u +%Y-%m-%dT%H:%M:%SZ)] Deploy complete: $latest"
