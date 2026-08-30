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

### ✅ Session B6: Minimap Radar, Fog of War, Audio SFX & New Units
- [x] **Interactive Minimap Radar** ([`crates/client/src/minimap.rs`](file:///home/john/Godot/rts-bevy/crates/client/src/minimap.rs)):
  - Bottom-left radar widget with real-time entity blips (units, structures, crystal nodes).
  - Dynamic camera frustum viewport rectangle.
  - Left-click drag to pan camera and right-click minimap order routing.
- [x] **Fog of War** ([`crates/client/src/fog_of_war.rs`](file:///home/john/Godot/rts-bevy/crates/client/src/fog_of_war.rs)):
  - Grid-based vision state machine (`Unexplored` $\to$ `Explored` $\to$ `Visible`).
  - Dynamic unit and structure sight radiuses with viewport-culled rendering.
  - Hides hostile entities under fog of war.
- [x] **Web Audio SFX Engine** ([`crates/client/src/audio_sfx.rs`](file:///home/john/Godot/rts-bevy/crates/client/src/audio_sfx.rs)):
  - Web Audio synth audio for gunshots, laser mining, build placement, unit training, and victory/defeat fanfares.
- [x] **Expanded Unit Roster & Defenses**:
  - **Gun Turret**: Automated stationary base defense with rapid fire ([`crates/client/src/combat.rs`](file:///home/john/Godot/rts-bevy/crates/client/src/combat.rs), [`crates/server/src/sim_systems.rs`](file:///home/john/Godot/rts-bevy/crates/server/src/sim_systems.rs)).
  - **Siege Tank**: Heavy armored tracked vehicle with rotating artillery cannon.
  - Full client and dedicated server simulation parity for all combat units.

---

### 🛡️ Dependency & Security Audit (August 20, 2026 Incident)
- [x] **Supply Chain Audit Verified**:
  - `arrayref` locked to safe version `0.3.9` with SHA-256 integrity verification.
  - Zero presence of compromised packages (`arrayref@0.3.10`, `internment@0.8.7`, `append-only-vec@0.1.9`, `proc-macro1`).
  - Automated security audit script created: [`scripts/security_audit.sh`](file:///home/john/Godot/rts-bevy/scripts/security_audit.sh).

---

### ✅ Session B7: A* Pathfinding, Tactical Command Stances & Tech Tree/Abilities
- [x] **A* Navigation Grid & Pathfinding** ([`crates/shared/src/grid.rs`](file:///home/john/Godot/rts-bevy/crates/shared/src/grid.rs)):
  - 64×64 cell navigation grid over 3200×3200 world space with dynamic building/obstacle blocking.
  - 8-directional A* search with Euclidean heuristic, diagonal corner-cut prevention, nearest walkable coordinate fallbacks, and line-of-sight shortcutting (string pulling).
  - Waypoint-based progression in [`MoveTarget`](file:///home/john/Godot/rts-bevy/crates/shared/src/components.rs) synchronized across client and dedicated server.
- [x] **Tactical Command Stances & Hotkeys** ([`crates/client/src/command_marker.rs`](file:///home/john/Godot/rts-bevy/crates/client/src/command_marker.rs), [`crates/client/src/unit_movement.rs`](file:///home/john/Godot/rts-bevy/crates/client/src/unit_movement.rs), [`crates/shared/src/protocol.rs`](file:///home/john/Godot/rts-bevy/crates/shared/src/protocol.rs)):
  - **Stop (`S`)**: Halts all movement and clears attack targets immediately.
  - **Hold Position (`H`)**: Anchors units in place, firing at enemies within attack range without chasing.
  - **Patrol (`P`)**: Orders units to cycle back and forth along a route in aggressive attack-move stance.
  - **Attack-Move (`A`)**: Aggressive movement engaging any enemy spotted along the path.
- [x] **Tech Tree Prerequisites & Unit Abilities**:
  - **Prerequisites**: Gun Turret requires a completed Barracks; Siege Tank requires completed Barracks + Supply Depot.
  - **Marine Stimpack (`T`)**: Sacrifices 15 HP for +50% move speed and +50% fire rate for 6.0s with glowing crimson aura visual.
  - **Siege Tank Siege Mode (`E`)**: Transforms mobile tank into long-range stationary artillery (380px range, 70 DMG + 45px area splash damage) with deployed stabilizer struts.

---

### ✅ Session B8: Advanced Soundscape, Particle FX & Post-Game Scoreboard
- [x] **Particle FX Emitters** ([`crates/client/src/particles.rs`](file:///home/john/Godot/rts-bevy/crates/client/src/particles.rs)):
  - Explosions, smoke trails, bullet tracer lines, debris shrapnel, and damage spark emitters.
  - Performance-tuned lifetime-decay physics with particle pooling.
- [x] **Web Audio SFX Engine** ([`crates/client/src/audio_sfx.rs`](file:///home/john/Godot/rts-bevy/crates/client/src/audio_sfx.rs)):
  - Procedural sound synthesis for gunfire, siege artillery cannon shots, stimpack injection, mining lasers, build placement, and victory/defeat stingers.
- [x] **Match Statistics & End-Game Scoreboard** ([`crates/client/src/stats.rs`](file:///home/john/Godot/rts-bevy/crates/client/src/stats.rs), [`crates/client/src/ui.rs`](file:///home/john/Godot/rts-bevy/crates/client/src/ui.rs)):
  - Real-time tracking of minerals mined, units trained, units killed, units lost, buildings constructed, and damage dealt.
  - Tactical post-game victory/defeat debriefing screen with breakdown comparison graphs.

---

### ✅ Session B9: DevOps, VPS Hardening & Automated GitHub Actions CI/CD
- [x] **Production VPS Security Hardening (Ubuntu 24.04)**:
  - UFW firewall configured (ports 22, 80, 443 allowed; default deny incoming).
  - Fail2ban intrusion prevention with automated IP bans for SSH brute-force protection.
  - SSH key-only authentication (`PasswordAuthentication no`).
  - 1 GB swapfile configured to prevent Out-Of-Memory errors on 1 GB RAM instance.
- [x] **Zero-Host-Overhead Containerization**:
  - `docker/Dockerfile.server` (Ubuntu 24.04 minimal runtime with pre-built binary).
  - `docker/Dockerfile.client` (Caddy 2 Alpine serving pre-built Wasm client with automatic Let's Encrypt SSL/TLS).
  - `docker-compose.yml` with internal bridge network and localhost-isolated game server (port 8080 bound strictly to `127.0.0.1`).
- [x] **Automated GitHub Actions CI/CD Pipeline** ([`.github/workflows/deploy.yml`](file:///home/john/Godot/rts-bevy/.github/workflows/deploy.yml)):
  - Automated testing & security audit on every push to `main`.
  - Fast Wasm & Server binary compilation in GitHub Actions runners (saving VPS CPU & memory).
  - SSH deployment to UpCloud VPS with automated container restart and `/health` + `/api/stats` telemetry verification.
- [x] **Live Production Deployment**:
  - Live VPS running at UpCloud (Helsinki datacenter).
  - Production subdomain: **`https://rts.farell.ax`** (with direct IP fallback).

---

## 🎯 Next Steps & Future Roadmap

### 🗺️ Planned Session B10: Custom Lobbies, Player Profiles & In-Game Chat
- [ ] **Custom Game Lobbies**:
  - 4-digit room codes to invite friends directly to 1v1 PvP matches.
  - Player name customization and faction color selector (Blue, Red, Teal, Orange, Purple).
- [ ] **In-Game Tactical Chat**:
  - Real-time text chat box (`Enter` to toggle, `All` and `Team` channels).
  - System broadcast messages for player connections/disconnections.
- [ ] **Player Ping & Minimap Alert Markers**:
  - `Alt + Click` tactical ping markers on the terrain and minimap (Defend, Attack, Caution).

### 🗺️ Planned Session B11: Advanced Maps, Terrain Chokepoints & Destructibles
- [ ] **Multi-Tile Terrain & Obstacle System**:
  - Natural choke points, cliffs, ramps, and resource-rich valleys.
  - Destructible debris rocks (can be destroyed to open flanking pathways).
- [ ] **Map Selection**:
  - Map 1: *The Crucible* (Symmetrical 1v1 tournament arena with narrow center bridge).
  - Map 2: *Crystal Basin* (Open macro map with rich expansion mineral nodes).
- [ ] **Replay & Spectator Mode**:
  - Record match command streams for post-game playback analysis.

---

## ⚡ Quick Run & Management Commands

### Local Development:
```bash
# 1. Run security audit & test suite:
./scripts/security_audit.sh
cargo test --workspace

# 2. Run local web client (Wasm on port 8000):
trunk serve --open

# 3. Start local dedicated server:
cargo run --bin server
```

### Production Deployment:
```bash
# Push to main to trigger full automated CI/CD build and deploy:
git push origin main
```

