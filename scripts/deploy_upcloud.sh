#!/usr/bin/env bash
set -euo pipefail

# ─────────────────────────────────────────────────────────────
# Mini-RTS (Bevy 0.15 + Rust) - UpCloud VPS Deployment Script
# ─────────────────────────────────────────────────────────────

VPS_HOST="${VPS_HOST:-your-vps-ip-here}"
VPS_USER="${VPS_USER:-root}"
REMOTE_DIR="/opt/rts-bevy"

echo "🚀 [Deploy] Deploying Mini-RTS Bevy to UpCloud VPS ($VPS_USER@$VPS_HOST)..."

# 1. Ensure local tests pass
echo "🧪 [1/4] Running cargo test suite..."
cargo test --workspace

# 2. Synchronize project directory to VPS (excluding target directories)
echo "📦 [2/4] Syncing project files to VPS..."
rsync -avz --delete \
    --exclude 'target/' \
    --exclude '.git/' \
    --exclude 'dist/' \
    ./ "$VPS_USER@$VPS_HOST:$REMOTE_DIR"

# 3. Build & Run containers on VPS
echo "🐳 [3/4] Building and launching Docker Compose containers on VPS..."
ssh "$VPS_USER@$VPS_HOST" "bash -c '
    cd $REMOTE_DIR
    docker compose down || true
    docker compose build --pull
    docker compose up -d
'"

# 4. Verify deployment health
echo "🩺 [4/4] Performing health check..."
ssh "$VPS_USER@$VPS_HOST" "bash -c '
    docker compose ps
    curl -I --fail http://127.0.0.1:8000 || exit 1
'"

echo "🎉 [Deploy] Mini-RTS successfully deployed to http://$VPS_HOST:8000"
