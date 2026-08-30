# 🚀 Mini-RTS (Bevy 0.15) — UpCloud Deployment Guide

Denna guide sammanfattar hur vi driftsätter **Mini-RTS** till en **UpCloud VPS** med domänen `rts.farell.ax`.

---

## 📋 1. Förberedelser & Serverval på UpCloud.com

1. Logga in eller skapa konto på [upcloud.com](https://upcloud.com).
2. Klicka på **Deploy Server**:
   - **Plats / Datacenter**: `Helsinki (FI-HEL1 / FI-HEL2)` eller `Stockholm (SE-STO1)` *(ger <5–10 ms responstid)*.
   - **Plan**: **Simple Plan -> 1 vCPU / 1 GB RAM / 25 GB MaxIOPS** *(~€5.50 – €6.00/mån)*.
   - **Operativsystem**: **Ubuntu 24.04 LTS** (eller Debian 12).
   - **Autentisering**: Din SSH-nyckel (rekommenderas) eller lösenord.
   - **Hostname**: `mini-rtx-prod`.
3. Klicka på **Deploy**. Notera den **publika IP-adressen** (t.ex. `185.20.xx.xx`).

---

## 🌐 2. DNS-konfiguration för Domänen

Hos din domänhanterare för `farell.ax`:
- Skapa ett **A-record** för subdomänen `rts` -> `<DIN_SERVER_IP>`

*(DNS-uppdateringen brukar ta mellan 2 till 15 minuter att slå igenom).*

---

## ⚡ 3. Snabbinitiering & Säkerhetshärdning av VPS:en (Körs en gång via SSH)

När servern är startad på UpCloud (Ubuntu 24.04 LTS):

```bash
# 1. Logga in på servern
ssh root@<DIN_SERVER_IP>

# 2. Konfigurera brandväggen (UFW)
ufw default deny incoming
ufw default allow outgoing
ufw allow 22/tcp
ufw allow 80/tcp
ufw allow 443/tcp
ufw --force enable

# 3. Installera Fail2ban (skydd mot SSH brute-force)
apt update && apt install -y fail2ban
echo -e "[DEFAULT]\nbantime = 1h\nfindtime = 10m\nmaxretry = 5\nbackend = systemd\n\n[sshd]\nenabled = true\nport = 22" > /etc/fail2ban/jail.local
systemctl restart fail2ban

# 4. Skapa 1 GB Swap-fil (sparar diskutrymme på 10 GB disk och förhindrar OOM)
fallocate -l 1G /swapfile
chmod 600 /swapfile
mkswap /swapfile
swapon /swapfile
echo '/swapfile none swap sw 0 0' >> /etc/fstab

# 5. Installera officiella Docker Engine
curl -fsSL https://get.docker.com | sh
systemctl enable --now docker

# 6. Skapa SSH-deploynyckel för GitHub Actions
ssh-keygen -t ed25519 -C "github-actions-deploy" -f /root/.ssh/github_actions -N ""
cat /root/.ssh/github_actions.pub >> /root/.ssh/authorized_keys
cat /root/.ssh/github_actions   # Kopiera detta till GitHub Secrets (SSH_PRIVATE_KEY)

# 7. Logga ut
exit
```

---

## 🚢 4. Automatiserad Driftsättning via GitHub Actions (CI/CD)

Varje gång du pushar till `main` från din dator:
```bash
git push origin main
```

### Vad som sker i molnet:
1. 🧪 **GitHub Actions** kör säkerhetsgranskning (`./scripts/security_audit.sh`) och alla enhetstester (`cargo test --workspace`).
2. 📦 **Bygger Wasm & Server i CI**: Trunk genererar WebAssembly-klienten och Cargo kompilerar den dedikerade Linux-servern på GitHubs snabba servrar.
3. 🚀 **Synk & Driftsättning**: De färdiga artefakterna synkas direkt till `/opt/rts-bevy/` på servern.
4. 🐳 **Docker Start**: Startar `rts-bevy-web` (Caddy) och `rts-bevy-server` på 3 sekunder.
5. 🔒 **Automatisk SSL/TLS**: Caddy hämtar och förnyar gratis Let's Encrypt HTTPS-certifikat för `https://rts.farell.ax`.
6. 🩺 **Hälsokontroll**: Verifierar att `/health` och `/api/stats` svarar korrekt.

---

## 📊 5. Verifiera & Testa i Webbläsaren

Efter att scriptet är klart kan du surfa in på:
- **`https://rts.farell.ax`** (eller `http://<DIN_SERVER_IP>`)

### Kontrollera API och Telemetri:
- **Server Health**: `curl https://rts.farell.ax/health` -> `{"status":"ok"}`
- **Live Matchstatistik**: `curl https://rts.farell.ax/api/stats`
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

## 🛠️ 6. Nyttiga Driftkommandon på Servern

Om du behöver inspektera servern senare:

```bash
# Logga in
ssh root@<DIN_SERVER_IP>
cd /opt/rts-bevy

# Visa status för containrarna
docker compose ps

# Se realtidsloggar från spelsimuleringen
docker compose logs -f game-server

# Se webbserverns loggar (Caddy / SSL)
docker compose logs -f web-client

# Starta om servrarna
docker compose restart

# Stoppa allt
docker compose down
```

---

## 🎯 Sammanfattning av Kapacitet & Gränser
- **Max 10 samtidiga 1v1 PvP-matcher** (20 spelare).
- **Max 10 samtidiga Solo vs AI-matcher** (10 spelare).
- **Minnesåtgång**: Endast ~250–300 MB av 1024 MB RAM vid full belastning.
