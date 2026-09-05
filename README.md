# ⚡ Mini-RTS // Tactical Command (Rust & Bevy 0.15)

A high-performance, competitive 2D Real-Time Strategy game written in **Rust** using **Bevy 0.15**, compiled to **WebAssembly / WebGL 2.0** for instant browser play, and supported by a native client and an authoritative headless Linux dedicated server.

---

## 🎮 Features

- **Authoritative Headless Server**: 30 Hz deterministic simulation loop with multi-room matchmaking (Solo vs AI and 1v1 PvP).
- **WebAssembly & WebGL 2.0**: Zero-plugin browser deployment optimized for desktop Chrome, Firefox, and Edge.
- **Competitively Mirrored 1v1 Map ("Iron Meridian")**: 180° point-symmetric terrain with cliff ridges, ramp choke bluffs, natural expansions, and contested mineral deposits.
- **A\* Pathfinding & Steering**: 64×64 navigation grid with 8-directional movement, obstacle avoidance, nearest-walkable fallback, and line-of-sight shortcutting.
- **Dynamic Economy & Production**: Contextual SCV mineral mining with cyan pulsating lasers, supply mechanics, rally points, and structured production queues.
- **Tech Tree & Abilities**:
  - **Marine**: Automatic aggro scanning, rapid tracer fire, and **Stimpack (`T`)** (+50% speed / +50% fire rate for 6s at the cost of 15 HP).
  - **Siege Tank**: Mobile tracked tank with transformable **Siege Mode (`E`)** (long-range stationary artillery with splash damage).
  - **Gun Turret**: Stationary automated perimeter defense requiring an active Barracks.
- **Tactical Stances**: Stop (`S`), Hold Position (`H`), Patrol (`P`), and Attack-Move (`A`).
- **Real-Time Soundscape & FX**: Procedural Web Audio synthesizer sound effects and particle emitters for muzzle flashes, tracer rounds, and explosions.
- **Tactical Multiplayer Features**: Real-time in-game chat (`Enter`), terrain/minimap pings (`Alt + Click`), custom 4-digit room codes, and post-match scoreboard analytics.

---

## 🏗️ Architecture & Crates

The repository is structured as a modular Cargo workspace:

```
rts-bevy/
├── crates/
│   ├── shared/     # Shared components, protocol messages, A* grid, map layout, economy
│   ├── client/     # Bevy 2D client (Wasm & native), rendering, UI HUD, audio, camera, fog of war
│   ├── server/     # Dedicated headless server, Tokio async WebSocket listener, room matchmaking
│   └── bot_ai/     # Solo skirmish escalating assault wave logic
├── docker/         # Dockerfiles for dedicated server and Caddy web client
├── scripts/        # Security audit and deployment automation scripts
└── dist/           # Compiled WebAssembly production distribution bundle
```

---

## 🕹️ Controls

| Action | Control / Hotkey |
| :--- | :--- |
| **Camera Pan** | `W`, `A`, `S`, `D` or Screen Edge Scrolling |
| **Camera Zoom** | Mouse Scroll Wheel |
| **Select Units** | Left-Click or Drag Box (Hold `Shift` to add to selection) |
| **Issue Orders** | Right-Click (Move / Attack / Harvest minerals) |
| **Stop / Hold Position**| `S` (Stop), `H` (Hold Position) |
| **Patrol / Attack-Move** | `P` (Patrol), `A` (Attack-Move) |
| **Abilities** | `T` (Marine Stimpack), `E` (Tank Siege Mode) |
| **Build Menu** | `B` (Barracks: 150💎, Supply Depot: 100💎, Gun Turret: 125💎, Base HQ: 400💎) |
| **Train Units** | `V` (SCV Worker at HQ), `M` (Marine at Barracks), `E` (Siege Tank at Barracks) |
| **Tactical Ping** | `Alt + Left Click` on terrain or minimap |
| **Chat** | `Enter` to open/send chat, `Esc` to cancel |
| **Menu / Lobby** | `Tab` or `F1` |

---

## 🚀 Getting Started

### 1. Prerequisites

- **Rust**: Stable toolchain (`1.80+`) with `wasm32-unknown-unknown` target:
  ```bash
  rustup target add wasm32-unknown-unknown
  ```
- **Trunk**: WebAssembly bundler:
  ```bash
  cargo install trunk
  ```
- **Docker & Docker Compose**: (Optional, for containerized execution).

---

### 2. Local Development with Docker (Recommended)

To run both the dedicated server and the web client via Docker Compose:

```bash
# 1. Build release binaries
cargo build --release --bin server
trunk build --release

# 2. Prepare deployment binary
mkdir -p bin && cp target/release/server bin/server

# 3. Start containers
docker compose up -d --build
```

Access the game at:
- **Client**: [http://localhost:8088](http://localhost:8088)
- **Health Check**: [http://localhost:8088/health](http://localhost:8088/health)
- **Telemetry API**: [http://localhost:8088/api/stats](http://localhost:8088/api/stats)

*(Configuration options such as `WEB_PORT`, `SSL_PORT`, and `RTS_DOMAIN` can be set in `.env`)*.

---

### 3. Native & Standalone Development

You can also run the client and server directly without Docker:

```bash
# Start the dedicated server (port 8080)
cargo run --bin server

# In a separate terminal, serve the Web client (port 8000)
trunk serve --open
```

Or run the client natively on desktop:
```bash
cargo run --bin client
```

---

## 🧪 Testing & Verification

Run the comprehensive unit test suite and security audit:

```bash
# Run all workspace unit tests
cargo test --workspace

# Run dependency & supply-chain security audit
./scripts/security_audit.sh
```

---

## 🌐 Production Deployment

The project includes an automated **GitHub Actions CI/CD pipeline** ([`.github/workflows/deploy.yml`](.github/workflows/deploy.yml)) that:
1. Runs workspace unit tests and security checks.
2. Compiles the WebAssembly client and server binary on GitHub runners.
3. Deploys lightweight Docker containers to an UpCloud VPS behind Caddy with automated HTTPS.

For full VPS configuration details, refer to [deployment.md](deployment.md).
