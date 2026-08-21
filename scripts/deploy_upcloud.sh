#!/usr/bin/env bash
set -euo pipefail

# ─────────────────────────────────────────────────────────────
# Mini-RTS (Bevy 0.15 + Rust) - UpCloud VPS Deployment Script
# Usage: ./scripts/deploy_upcloud.sh [VPS_IP] [DOMAIN]
# Example: ./scripts/deploy_upcloud.sh 185.20.12.34 mini-rtx.ax
# ─────────────────────────────────────────────────────────────

VPS_HOST="${1:-${VPS_HOST:-}}"
DOMAIN="${2:-${RTS_DOMAIN:-mini-rtx.ax}}"
VPS_USER="${VPS_USER:-root}"
REMOTE_DIR="/opt/rts-bevy"

if [ -z "$VPS_HOST" ]; then
    echo "❌ Error: VPS IP address required."
    echo "Usage: $0 <VPS_IP> [DOMAIN]"
    echo "Example: $0 185.20.12.34 mini-rtx.ax"
    exit 1
fi

echo "================================================================"
echo "🚀 [Deploy] Deploying Mini-RTS Bevy to UpCloud VPS ($VPS_USER@$VPS_HOST)"
echo "🌐 [Domain] $DOMAIN"
echo "================================================================"

# 1. Run local tests & security audit
echo "🧪 [1/5] Running workspace unit tests & security checks..."
cargo test --workspace
./scripts/security_audit.sh

# 2. Prepare remote directory
echo "📁 [2/5] Preparing remote deployment directory at $REMOTE_DIR..."
ssh -o StrictHostKeyChecking=accept-new "$VPS_USER@$VPS_HOST" "mkdir -p $REMOTE_DIR"

# 3. Synchronize project directory to VPS
echo "📦 [3/5] Syncing project files to VPS..."
rsync -avz --delete \
    --exclude 'target/' \
    --exclude '.git/' \
    --exclude 'dist/' \
    ./ "$VPS_USER@$VPS_HOST:$REMOTE_DIR"

# 4. Build & Run containers on VPS
echo "🐳 [4/5] Building and launching Docker Compose containers on VPS..."
ssh "$VPS_USER@$VPS_HOST" "bash -c '
    cd $REMOTE_DIR
    export RTS_DOMAIN=\"$DOMAIN\"
    docker compose down || true
    docker compose build
    docker compose up -d
'"

# 5. Verify deployment health
echo "🩺 [5/5] Performing live telemetry health check..."
sleep 3
ssh "$VPS_USER@$VPS_HOST" "bash -c '
    docker compose ps
    echo \"Checking HTTP health...\"
    curl -s --fail http://127.0.0.1:8080/health || exit 1
    echo \"Checking API stats...\"
    curl -s http://127.0.0.1:8080/api/stats
    echo \"\"
'"

echo "================================================================"
echo "🎉 Mini-RTS successfully deployed and running on UpCloud!"
echo "🌐 URL: https://$DOMAIN (or http://$VPS_HOST)"
echo "================================================================"

