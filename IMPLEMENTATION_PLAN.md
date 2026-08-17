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

### ✅ Session B5: Dedicated Server, Multiplayer Networking & VPS Deployment
- [x] Protocol & Serialization ([`crates/shared/src/protocol.rs`](file:///home/john/Godot/rts-bevy/crates/shared/src/protocol.rs)):
  - `ClientMessage` (`JoinLobby`, `RequestMove`, `RequestAttackTarget`, `RequestHarvest`, `RequestBuild`, `RequestTrainUnit`, `RequestSetRallyPoint`, `RequestStop`, `RequestHoldPosition`, `Ping`).
  - `ServerMessage` (`LobbyJoined`, `GameStarted`, `InitialWorldState`, `TickSnapshotBatch`, `BuildingSpawned`, `UnitSpawned`, `QueueUpdated`, `ProjectileFired`, `EntityDamaged`, `EntityDied`, `MatchEnded`, `Pong`).
  - Binary serialization / deserialization helpers using `bincode`.
- [x] Authoritative Dedicated Server ([`crates/server`](file:///home/john/Godot/rts-bevy/crates/server)):
  - Tokio async WebSocket listener on port 8080 with crossbeam channel ECS bridging.
  - Multi-room matchmaking (Solo vs AI practice or 1v1 PvP matching).
  - 30 Hz authoritative simulation for movement, soft Boids separation, mining, production, and combat damage.
  - 30 Hz state snapshot broadcasting to connected clients.
- [x] Client Network Synchronization ([`crates/client/src/net.rs`](file:///home/john/Godot/rts-bevy/crates/client/src/net.rs)):
  - Cross-platform WebSocket client using `ewebsock` (Native and WebAssembly / WebGL2).
  - Client-side prediction for instant responsive feedback on movement/build/train orders.
  - 30 Hz snapshot reconciliation & entity lerp interpolation.
  - Live latency / ping and connection status indicator in HUD.
- [x] Containerization & VPS Deployment:
  - `docker/Dockerfile.server` (Minimal multi-stage Debian server image).
  - `docker/Dockerfile.client` (Trunk WebAssembly builder + Caddy static web server).
  - `docker/Caddyfile` & `docker-compose.yml`.
  - `scripts/deploy_upcloud.sh` (Automated SSH & Docker Compose deployment to UpCloud VPS).

---

## 🎯 Next Session: Session B6 (Future Polish & Systems)

### Focus: Interactive Minimap Radar, Fog of War & Audio Effects

```
+-------------------------------------------------------------------------+
|                              SESSION B6                                 |
+-------------------------------------------------------------------------+
|                                                                         |
|  [Part 1: Minimap Radar]   Interactive 2D radar widget with frustum box |
|                            and click-to-pan camera navigation           |
|                                                                         |
|  [Part 2: Fog of War]      Grid-based vision exploration system         |
|                            (Unexplored / Explored / Visible)            |
|                                                                         |
|  [Part 3: Audio Effects]   Kira audio plugin integration for SFX        |
|                            (Shots, clicks, mining laser, alerts)        |
|                                                                         |
|  [Part 4: New Units]       Siege Tank & Defensive Gun Turret            |
|                                                                         |
+-------------------------------------------------------------------------+
```

---

## ⚡ Quick Run Commands

To run locally:

```bash
# 1. Start dedicated server (port 8080):
cargo run --bin server

# 2. Run local web client (Wasm on port 8000):
trunk serve --open

# 3. Or run native desktop client:
cargo run --bin client

# 4. Or launch entire stack in Docker:
docker compose up --build
```

