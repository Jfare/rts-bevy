# Mini-RTS (Bevy 0.15 + Rust) - Project Status & Next Steps Plan

## 📌 Executive Summary
This project is a high-performance **2D Mini-RTS** written in **Rust** using **Bevy 0.15**, compiled to **WebAssembly / WebGL2** for seamless browser play, with a native desktop client and a lightweight dedicated headless Linux server.

All in-game text, UI, HUD elements, unit cards, and commands are strictly in **English**.

---

## 🏆 Current Progress & Completed Sessions

### ✅ Session B1: Foundation & Architecture Setup
- [x] Multi-crate workspace configured: `crates/shared`, `crates/client`, `crates/server`, `crates/bot_ai`.
- [x] Rust `wasm32-unknown-unknown` toolchain & Trunk 0.21 build pipeline.
- [x] Core ECS component definitions with Serde & Bevy Reflect:
  - `Faction`, `Health`, `Unit`, `Worker`, `Soldier`, `Building`, `BaseHQ`, `Barracks`, `SupplyDepot`, `ResourceNode`, `Selectable`, `NetEntity`.
- [x] Dedicated headless server setup with 30 Hz deterministic tick loop.
- [x] Bevy Remote Protocol (BRP) MCP integration on port 15702 for real-time live debugging.

### ✅ Session B2: Tactical 2D Map, Camera & Unit Controls
- [x] Procedural infinite world grid with 3200x3200 bounds and coordinate markers.
- [x] 2D RTS Camera with WASD pan, screen edge scrolling, smooth mouse-wheel zoom, and boundary clamping.
- [x] Single-click & marquee drag-box selection with Shift additive modifiers and green/red tactical rings.
- [x] Tactical right-click command marker with golden angle formation spreading.
- [x] Smooth unit steering orientation and translation.

### ✅ Session B3: Economy, SCV Harvesting & Building Loop
- [x] Global `PlayerEconomy` resource tracking per-faction minerals, current supply, and max supply.
- [x] Contextual SCV harvesting cycle:
  - Right-click mineral patch $\to$ travel to crystal $\to$ 1.8s mining with **cyan pulsating laser (`#38bdf8`)** $\to$ carry mineral diamond $\to$ deposit +10 💎 at Base HQ $\to$ return to patch.
- [x] 16px Grid Ghost Building Placement:
  - Hotkeys: `B` (Barracks - 150 💎), `P` (Supply Depot - 100 💎), `H` (Base HQ - 400 💎).
  - Green / Red validity feedback with overlap collision prevention.
  - Multi-placement support with Shift+Click.
- [x] Construction & Production Queues:
  - Base HQ: Train SCV Worker (`V` key, 50 💎, 1 ⚡, 3.0s).
  - Barracks: Train Marine Soldier (`M` key, 100 💎, 2 ⚡, 4.0s).
  - Supply Depot: Automatically grants **+8 Max Supply** upon completion.
  - Right-click rally point routing for newly produced units.

### ✅ Session B4: Combat System & Solo Skirmish AI Waves
- [x] Marine Combat State Machine:
  - Automatic scanning for hostile units within 240px aggro radius every 0.15s.
  - 150px firing range with smooth aiming and 0.85s firing cadence.
  - Right-click focus fire targeting on enemy units and structures.
- [x] Projectiles & FX:
  - High-speed (780 px/s) golden tracer rounds with trail lines.
  - Incandescent muzzle flashes at gun barrels.
  - Direct health deduction upon bullet collision and entity elimination at 0 HP.
- [x] Unit Separation & Obstacle Collision:
  - Soft elastic Boids separation pushing overlapping units apart smoothly.
  - Pushback against buildings and mineral nodes to prevent clipping.
- [x] Solo Skirmish Escalating Assault Waves (`bot_ai`):
  - Wave 1 (3 Marines) $\to$ Wave 2 (6 Marines) $\to$ Wave 3 (10 Marines) $\to$ Wave 4+ (14+ Marines).
  - Wave spawning automatically ceases when the enemy base is destroyed.
- [x] Match Outcome:
  - `🏆 VICTORY!` banner when Hostile AI Base HQ is destroyed.
  - `💥 DEFEAT!` banner when Player 1 Base HQ is lost.

---

## 🎯 Next Session: Session B5 (Plan of Action)

### Focus: Dedicated Server, Multiplayer Networking & VPS Deployment

```
+-------------------------------------------------------------------------+
|                              SESSION B5                                 |
+-------------------------------------------------------------------------+
|                                                                         |
|  [Part 1: Protocol]        crates/shared/src/protocol.rs                |
|                            - ClientCommand (Move, Attack, Build, Train) |
|                            - ServerSnapshot (Entity states, HP, Eco)    |
|                                                                         |
|  [Part 2: Server]          crates/server/src/main.rs                    |
|                            - Headless Bevy 0.15 app (30 Hz tick)        |
|                            - Async WebSocket listener (tokio / axum)    |
|                            - Authoritative combat, movement & economy   |
|                            - Room & Matchmaking management (1v1, Solo)  |
|                                                                         |
|  [Part 3: Client Sync]     crates/client/src/net.rs                     |
|                            - Wasm WebSocket client integration          |
|                            - Client-side prediction & reconciliation    |
|                            - Remote peer unit interpolation             |
|                            - Multiplayer Lobby UI in English            |
|                                                                         |
|  [Part 4: Deployment]      Docker & Linux VPS (UpCloud)                 |
|                            - Dockerfile.server (Minimal Alpine Rust)    |
|                            - Dockerfile.client (Nginx static web host)  |
|                            - docker-compose.yml                         |
|                            - scripts/deploy_upcloud.sh                  |
|                                                                         |
+-------------------------------------------------------------------------+
```

### Detailed Breakdown for Session B5:

#### 1. Binary Protocol Serialization (`crates/shared/src/protocol.rs`)
- Compact binary serialization using `bincode` and `serde`.
- `ClientMessage`:
  - `Connect { player_name: String }`
  - `CommandMove { unit_ids: Vec<u32>, target: Vec2, is_attack_move: bool }`
  - `CommandAttack { unit_ids: Vec<u32>, target_net_id: u32 }`
  - `CommandHarvest { worker_ids: Vec<u32>, node_net_id: u32 }`
  - `CommandBuild { kind: BuildingKind, position: Vec2 }`
  - `CommandTrain { building_net_id: u32, unit_name: String }`
  - `CommandRallyPoint { building_net_id: u32, target: Vec2 }`
- `ServerMessage`:
  - `Welcome { assigned_faction: Faction, player_id: u64 }`
  - `WorldSnapshot { tick: u64, entities: Vec<NetEntityState>, economy: FactionEconomy, wave: WaveInfo }`
  - `MatchEnd { outcome: MatchOutcome }`

#### 2. Authoritative Dedicated Server (`crates/server`)
- Listens on WebSocket port `8080`.
- Runs full Bevy ECS simulation without graphics:
  - Simulates movement, pathing, soft separation, resource deposits, and combat damage.
  - Broadcasts 30 Hz world state snapshots to all connected clients.
  - Supports 2 human players (PvP 1v1) or 1 human player vs Server AI.

#### 3. Client Multiplayer Synchronization (`crates/client`)
- Connects via WebSocket in WebAssembly (`web-sys` / `ws`) and Native Desktop.
- Applies local client prediction for instant responsive control on movement/selection.
- Smoothly interpolates positions of enemy/peer units based on server snapshots.
- Multiplayer lobby UI:
  - `[Play Solo Skirmish (vs AI)]`
  - `[Join 1v1 Multiplayer Match]`

#### 4. Containerization & UpCloud VPS Deployment
- **`Dockerfile.server`**: Multi-stage Rust build producing a minimal ~25 MB binary image.
- **`Dockerfile.client`**: Nginx image serving optimized Wasm/JS/HTML static assets with Gzip/Brotli.
- **`docker-compose.yml`**: Exposes port 80/443 for web client and port 8080 for WebSocket game server.
- **Deployment Script (`scripts/deploy_upcloud.sh`)**:
  - One-command SSH deployment: builds Docker images, transfers to VPS, and starts services with health checks.

---

## ⚡ Quick Resume Commands for Next Session

To resume right where we left off:

```bash
# 1. Run local web client (Wasm):
cd /home/john/Godot/rts-bevy
trunk serve --open

# 2. Run native desktop client:
cargo run --bin client

# 3. Run headless server:
cargo run --bin server
```
