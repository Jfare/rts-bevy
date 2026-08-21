# 🚀 Mini-RTS (Bevy 0.15) — UpCloud Deployment Guide

Denna guide sammanfattar hur vi driftsätter **Mini-RTS** till en **UpCloud VPS** med domänen `mini-rtx.ax` (eller `mini-rts.ax`).

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

Hos din domänhanterare för `mini-rtx.ax` (eller `mini-rts.ax`):
- Skapa ett **A-record** för `@` -> `<DIN_SERVER_IP>`
- Skapa ett **A-record** för `www` -> `<DIN_SERVER_IP>`

*(DNS-uppdateringen brukar ta mellan 2 till 15 minuter att slå igenom).*

---

## ⚡ 3. Snabbinitiering av VPS:en (Körs en gång via SSH)

När servern är startad, logga in och installera Docker samt konfigurera en 2 GB swap-fil (bra säkerhetsmarginal vid byggen på en 1 GB-maskin):

```bash
# 1. Logga in på servern
ssh root@<DIN_SERVER_IP>

# 2. Installera Docker & Docker Compose (officiellt snabbscript)
curl -fsSL https://get.docker.com | sh

# 3. Skapa en 2 GB swap-fil (rekommenderas för 1 GB RAM)
fallocate -l 2G /swapfile
chmod 600 /swapfile
mkswap /swapfile
swapon /swapfile
echo '/swapfile none swap sw 0 0' >> /etc/fstab

# 4. Logga ut
exit
```

---

## 🚢 4. Driftsättning med ett enda kommando!

Från ditt lokala projekt (`/home/john/Godot/rts-bevy`) kör du vårt färdiga deployment-script:

```bash
./scripts/deploy_upcloud.sh <DIN_SERVER_IP> mini-rtx.ax
```

### Vad scriptet gör automatiskt:
1. 🧪 Kör alla lokala tester (`cargo test --workspace`) och säkerhetsgranskningar.
2. 📁 Skapar målmappen `/opt/rts-bevy` på servern.
3. 📦 Synkar källkoden och Docker-filerna via `rsync`.
4. 🐳 Bygger och startar containrarna med `docker compose up -d`.
5. 🔒 **Caddy ordnar automatiskt gratis Let's Encrypt SSL/TLS** för `https://mini-rtx.ax`.
6. 🩺 Verifierar att `/health` och `/api/stats` svarar korrekt.

---

## 📊 5. Verifiera & Testa i Webbläsaren

Efter att scriptet är klart kan du surfa in på:
- **`https://mini-rtx.ax`** (eller `http://<DIN_SERVER_IP>`)

### Kontrollera API och Telemetri:
- **Server Health**: `curl https://mini-rtx.ax/health` -> `{"status":"ok"}`
- **Live Matchstatistik**: `curl https://mini-rtx.ax/api/stats`
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
