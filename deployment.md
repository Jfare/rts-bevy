# 🚀 Mini-RTS (Bevy 0.15) — UpCloud Deployment Guide

This guide summarizes how to deploy **Mini-RTS** to an **UpCloud VPS** under the domain `rts.farell.ax`.

---

## 📋 1. Preparation & Server Selection on UpCloud.com

1. Log in or create an account on [upcloud.com](https://upcloud.com).
2. Click **Deploy Server**:
   - **Location / Datacenter**: `Helsinki (FI-HEL1 / FI-HEL2)` or `Stockholm (SE-STO1)` *(provides <5–10 ms response time)*.
   - **Plan**: **Simple Plan -> 1 vCPU / 1 GB RAM / 25 GB MaxIOPS** *(~€5.50 – €6.00/month)*.
   - **Operating System**: **Ubuntu 24.04 LTS** (or Debian 12).
   - **Authentication**: Your SSH key (recommended) or password.
   - **Hostname**: `mini-rtx-prod`.
3. Click **Deploy**. Note the **public IP address** (e.g. `185.20.xx.xx`).

---

## 🌐 2. DNS Configuration for Domain

In your domain registrar / DNS provider for `farell.ax`:
- Create an **A-record** for subdomain `rts` -> `<YOUR_SERVER_IP>`

*(DNS propagation usually takes between 2 to 15 minutes).*

---

## ⚡ 3. VPS Initialization & Security Hardening (Run once via SSH)

Once the server is booted on UpCloud (Ubuntu 24.04 LTS):

```bash
# 1. Log in to the server
ssh root@<YOUR_SERVER_IP>

# 2. Configure firewall (UFW)
ufw default deny incoming
ufw default allow outgoing
ufw allow 22/tcp
ufw allow 80/tcp
ufw allow 443/tcp
ufw --force enable

# 3. Install Fail2ban (SSH brute-force protection)
apt update && apt install -y fail2ban
echo -e "[DEFAULT]\nbantime = 1h\nfindtime = 10m\nmaxretry = 5\nbackend = systemd\n\n[sshd]\nenabled = true\nport = 22" > /etc/fail2ban/jail.local
systemctl restart fail2ban

# 4. Create 1 GB Swapfile (conserves disk space on 10 GB disk and prevents OOM)
fallocate -l 1G /swapfile
chmod 600 /swapfile
mkswap /swapfile
swapon /swapfile
echo '/swapfile none swap sw 0 0' >> /etc/fstab

# 5. Install official Docker Engine
curl -fsSL https://get.docker.com | sh
systemctl enable --now docker

# 6. Generate SSH deploy key for GitHub Actions
ssh-keygen -t ed25519 -C "github-actions-deploy" -f /root/.ssh/github_actions -N ""
cat /root/.ssh/github_actions.pub >> /root/.ssh/authorized_keys
cat /root/.ssh/github_actions   # Copy this into GitHub Secrets (SSH_PRIVATE_KEY)

# 7. Log out
exit
```

---

## 🚢 4. Automated Deployment via GitHub Actions (CI/CD)

Every time you push to `main` from your computer:
```bash
git push origin main
```

### What happens in the cloud:
1. 🧪 **GitHub Actions** runs security verification (`./scripts/security_audit.sh`) and all workspace unit tests (`cargo test --workspace`).
2. 📦 **Builds Wasm & Server in CI**: Trunk compiles the WebAssembly client and Cargo compiles the dedicated Linux server on GitHub's runners.
3. 🚀 **Sync & Deploy**: The built artifacts are synced directly to `/opt/rts-bevy/` on the server.
4. 🐳 **Docker Launch**: Starts `rts-bevy-web` (Caddy) and `rts-bevy-server` in 3 seconds.
5. 🔒 **Automatic SSL/TLS**: Caddy automatically provisions and renews free Let's Encrypt HTTPS certificates for `https://rts.farell.ax`.
6. 🩺 **Health Check**: Verifies that `/health` and `/api/stats` respond with status 200 OK.

---

## 📊 5. Verification & Browser Testing

After the deployment workflow completes, visit:
- **`https://rts.farell.ax`** (or `http://<YOUR_SERVER_IP>`)

### Check API & Telemetry:
- **Server Health**: `curl https://rts.farell.ax/health` -> `{"status":"ok"}`
- **Live Match Telemetry**: `curl https://rts.farell.ax/api/stats`
  ```json
  {
    "queue_1v1": 0,
    "active_1v1_matches": 0,
    "max_1v1_matches": 10,
    "active_solo_matches": 0,
    "max_solo_matches": 10,
    "total_online": 0,
    "status": "online"
  }
  ```

---

## 🛠️ 6. Useful Server Management Commands

If you need to inspect the server manually:

```bash
# Log in
ssh root@<YOUR_SERVER_IP>
cd /opt/rts-bevy

# View container status
docker compose ps

# View real-time game simulation logs
docker compose logs -f game-server

# View web server logs (Caddy / SSL)
docker compose logs -f web-client

# Restart services
docker compose restart

# Stop all services
docker compose down
```

---

## 🎯 Capacity & Limits Summary
- **Max 10 concurrent 1v1 PvP matches** (20 players).
- **Max 10 concurrent Solo vs AI matches** (10 players).
- **Memory Footprint**: Only ~250–300 MB out of 1024 MB RAM under full load.
