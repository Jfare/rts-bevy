use bevy::prelude::*;
use shared::components::{
    Barracks, BaseHQ, Building, Faction, GunTurret, Health, MeleeFighter, Radius, ResourceNode,
    Selectable, Soldier, SupplyDepot, TacticalStance, Unit, Worker,
};
use shared::grid::WorldGridConfig;
use crate::fog_of_war::{FogOfWarGrid, FogState};
use crate::net::NetClient;

pub struct RenderUnitsPlugin;

impl Plugin for RenderUnitsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                draw_units_system,
                draw_buildings_system,
                draw_resources_system,
                draw_health_bars_system,
            ),
        );
    }
}

/// Renders units (Workers, Ranged Fighters, Melee Fighters) with faction colors, heading indicators, and weapons
fn draw_units_system(
    mut gizmos: Gizmos,
    fog: Res<FogOfWarGrid>,
    grid_cfg: Option<Res<WorldGridConfig>>,
    net_client: Res<NetClient>,
    query: Query<(
        &Transform,
        &Radius,
        &Faction,
        &Selectable,
        Option<&Worker>,
        Option<&Soldier>,
        Option<&MeleeFighter>,
        Option<&TacticalStance>,
    ), With<Unit>>,
) {
    let default_cfg = WorldGridConfig::default();
    let config = grid_cfg.as_deref().unwrap_or(&default_cfg);

    for (transform, radius, faction, selectable, worker_opt, soldier_opt, melee_opt, stance_opt) in &query {
        let pos = transform.translation.truncate();

        // Shroud hostile units outside active friendly vision
        if *faction != net_client.my_faction && *faction != Faction::Neutral
            && fog.get_state_at_world_pos(pos, config) != FogState::Visible {
                continue;
            }
        let r = radius.0;
        let rot = transform.rotation.to_euler(EulerRot::ZYX).0;

        let body_color = if *faction == net_client.my_faction {
            net_client.my_color.to_color()
        } else {
            let [cr, cg, cb, _] = faction.color_rgba();
            Color::srgb(cr, cg, cb)
        };
        let outline_color = body_color.lighter(0.25);

        // 1. Tactical Stance Indicators
        if let Some(stance) = stance_opt {
            match stance {
                TacticalStance::HoldPosition => {
                    gizmos.circle_2d(pos, r + 4.0, Color::srgba(0.25, 0.65, 1.0, 0.75));
                }
                TacticalStance::Patrol { origin, target, .. }
                    if selectable.is_selected => {
                        gizmos.line_2d(*origin, *target, Color::srgba(0.35, 0.75, 1.0, 0.75));
                        gizmos.circle_2d(*target, 6.0, Color::srgba(0.35, 0.75, 1.0, 0.85));
                    }
                _ => {}
            }
        }

        // Body Circle for all units
        gizmos.circle_2d(pos, r, body_color);
        gizmos.circle_2d(pos, r, outline_color);

        // Heading Direction Pointer
        let forward = Vec2::new(rot.cos(), rot.sin());
        let right_side = Vec2::new(-forward.y, forward.x);
        let tip = pos + forward * (r + 6.0);
        let left = pos + forward * (r - 2.0) + right_side * 4.0;
        let right = pos + forward * (r - 2.0) - right_side * 4.0;

        gizmos.line_2d(tip, left, Color::WHITE);
        gizmos.line_2d(tip, right, Color::WHITE);
        gizmos.line_2d(left, right, Color::WHITE);

        // Ranged Fighter Rifle Barrel
        if soldier_opt.is_some() {
            let gun_tip = pos + forward * (r + 10.0);
            let gun_base = pos + forward * (r + 2.0);
            gizmos.line_2d(gun_base, gun_tip, Color::srgb(0.9, 0.9, 0.95));
            gizmos.rect_2d(gun_tip, Vec2::splat(3.0), Color::srgb(0.7, 0.7, 0.8));
        }

        // Worker Welder Arms
        if worker_opt.is_some() {
            let arm_left = pos + forward * (r + 4.0) + right_side * 5.0;
            let arm_right = pos + forward * (r + 4.0) - right_side * 5.0;
            gizmos.circle_2d(arm_left, 2.5, Color::srgb(0.95, 0.75, 0.20));
            gizmos.circle_2d(arm_right, 2.5, Color::srgb(0.95, 0.75, 0.20));
        }

        // Melee Fighter: Carried Sword & Dynamic Slash Swing Animation!
        if let Some(melee) = melee_opt {
            // Metallic shoulder pads / armor trim
            let left_shoulder = pos + right_side * (r * 0.85);
            let right_shoulder = pos - right_side * (r * 0.85);
            gizmos.circle_2d(left_shoulder, 3.5, Color::srgb(0.85, 0.88, 0.92));
            gizmos.circle_2d(right_shoulder, 3.5, Color::srgb(0.85, 0.88, 0.92));

            // Dynamic sword strike when swinging, or resting sword at right hand when idle/moving
            if melee.swing_timer > 0.0 {
                // Swing progress from 0.0 (just started) to 1.0 (finished)
                let swing_progress = 1.0 - (melee.swing_timer / 0.18).clamp(0.0, 1.0);
                // Sword sweeps dynamically across from right (+50 deg) to forward-left (-65 deg)
                let swing_angle = rot + 0.9 - swing_progress * 2.0;
                let blade_dir = Vec2::new(swing_angle.cos(), swing_angle.sin());
                let hilt_pos = pos + blade_dir * (r * 0.6);
                let sword_tip = pos + blade_dir * (r + 20.0);

                // Crossguard
                let cross_dir = Vec2::new(-blade_dir.y, blade_dir.x);
                gizmos.line_2d(
                    hilt_pos + cross_dir * 5.0,
                    hilt_pos - cross_dir * 5.0,
                    Color::srgb(0.85, 0.70, 0.20),
                );

                // Gleaming sword blade
                gizmos.line_2d(hilt_pos, sword_tip, Color::srgb(1.0, 1.0, 1.0));

                // Bright slashing energy arc in front of the unit
                let arc_radius = r + 18.0;
                let arc_segments = 6;
                let start_a = rot - 1.1;
                let end_a = rot + 0.9;
                let step = (end_a - start_a) / (arc_segments as f32);
                for s in 0..arc_segments {
                    let a1 = start_a + (s as f32) * step;
                    let a2 = start_a + ((s + 1) as f32) * step;
                    let p1 = pos + Vec2::new(a1.cos(), a1.sin()) * arc_radius;
                    let p2 = pos + Vec2::new(a2.cos(), a2.sin()) * arc_radius;
                    let alpha = 0.85 * (1.0 - swing_progress);
                    gizmos.line_2d(p1, p2, Color::srgba(0.40, 0.85, 1.0, alpha));
                }

                // Impact flash at sword tip
                gizmos.circle_2d(sword_tip, 4.0, Color::srgba(1.0, 0.95, 0.80, 0.9));
            } else {
                // Idle / marching sword carried at ready position
                let sword_base = pos - right_side * (r * 0.6) + forward * 2.0;
                let sword_tip = sword_base + forward * (r + 8.0) - right_side * 4.0;
                // Crossguard
                let hilt_mid = sword_base + forward * 4.0;
                gizmos.line_2d(
                    hilt_mid + right_side * 3.0,
                    hilt_mid - right_side * 3.0,
                    Color::srgb(0.85, 0.70, 0.20),
                );
                // Blade
                gizmos.line_2d(sword_base, sword_tip, Color::srgb(0.90, 0.92, 0.98));
            }
        }
    }
}

/// Renders buildings with distinct architectural silhouettes based on building type
fn draw_buildings_system(
    mut gizmos: Gizmos,
    fog: Res<FogOfWarGrid>,
    grid_cfg: Option<Res<WorldGridConfig>>,
    net_client: Res<NetClient>,
    query: Query<(
        &Transform,
        &Building,
        &Faction,
        Option<&BaseHQ>,
        Option<&Barracks>,
        Option<&SupplyDepot>,
        Option<&GunTurret>,
    )>,
) {
    let default_cfg = WorldGridConfig::default();
    let config = grid_cfg.as_deref().unwrap_or(&default_cfg);

    for (transform, building, faction, hq_opt, barracks_opt, supply_opt, turret_opt) in &query {
        let pos = transform.translation.truncate();

        // Shroud hostile buildings in unexplored fog
        if *faction != net_client.my_faction && *faction != Faction::Neutral {
            let fog_state = fog.get_state_at_world_pos(pos, config);
            if fog_state == FogState::Unexplored {
                continue;
            }
        }

        let size = building.size;

        let accent_col = if *faction == net_client.my_faction {
            net_client.my_color.to_color()
        } else {
            let [cr, cg, cb, _] = faction.color_rgba();
            Color::srgb(cr, cg, cb)
        };
        let base_col = Color::srgba(0.12, 0.16, 0.20, 0.95);

        // Base foundation box
        gizmos.rect_2d(pos, size, base_col);
        gizmos.rect_2d(pos, size, accent_col);

        if hq_opt.is_some() {
            // ─────────────────────────────────────────────────────────────
            // BASE HQ: Massive Command Fortress with Radar Dome
            // ─────────────────────────────────────────────────────────────
            gizmos.rect_2d(pos, size - Vec2::splat(12.0), accent_col.with_alpha(0.35));
            gizmos.circle_2d(pos, size.x * 0.25, accent_col);
            gizmos.circle_2d(pos, size.x * 0.14, Color::WHITE);
            // 4 Corner Antenna Pylons
            let offset = size * 0.38;
            gizmos.circle_2d(pos + Vec2::new(-offset.x, -offset.y), 4.0, Color::srgb(0.3, 0.8, 1.0));
            gizmos.circle_2d(pos + Vec2::new(offset.x, -offset.y), 4.0, Color::srgb(0.3, 0.8, 1.0));
            gizmos.circle_2d(pos + Vec2::new(-offset.x, offset.y), 4.0, Color::srgb(0.3, 0.8, 1.0));
            gizmos.circle_2d(pos + Vec2::new(offset.x, offset.y), 4.0, Color::srgb(0.3, 0.8, 1.0));
        } else if barracks_opt.is_some() {
            // ─────────────────────────────────────────────────────────────
            // BARRACKS: Armored Military Garrison with Dual Roof Cannons
            // ─────────────────────────────────────────────────────────────
            gizmos.rect_2d(pos, size - Vec2::splat(10.0), accent_col.with_alpha(0.3));
            let barrel_left_start = pos + Vec2::new(-16.0, 10.0);
            let barrel_left_end = pos + Vec2::new(-16.0, 32.0);
            let barrel_right_start = pos + Vec2::new(16.0, 10.0);
            let barrel_right_end = pos + Vec2::new(16.0, 32.0);

            gizmos.line_2d(barrel_left_start, barrel_left_end, Color::WHITE);
            gizmos.line_2d(barrel_right_start, barrel_right_end, Color::WHITE);
            gizmos.circle_2d(pos + Vec2::new(-16.0, 10.0), 6.0, accent_col);
            gizmos.circle_2d(pos + Vec2::new(16.0, 10.0), 6.0, accent_col);

            // Exit Bay Door at bottom
            gizmos.rect_2d(pos + Vec2::new(0.0, -size.y * 0.35), Vec2::new(28.0, 8.0), Color::srgb(0.95, 0.85, 0.25));
        } else if supply_opt.is_some() {
            // ─────────────────────────────────────────────────────────────
            // SUPPLY DEPOT: Power Generator with Glowing Energy Coils
            // ─────────────────────────────────────────────────────────────
            gizmos.circle_2d(pos, size.x * 0.32, Color::srgba(0.95, 0.75, 0.20, 0.4));
            gizmos.circle_2d(pos, size.x * 0.18, Color::srgb(0.95, 0.85, 0.25));
            let arm = size.x * 0.35;
            gizmos.line_2d(pos + Vec2::new(-arm, 0.0), pos + Vec2::new(arm, 0.0), Color::srgb(0.95, 0.85, 0.25));
            gizmos.line_2d(pos + Vec2::new(0.0, -arm), pos + Vec2::new(0.0, arm), Color::srgb(0.95, 0.85, 0.25));
        } else if let Some(turret) = turret_opt {
            // ─────────────────────────────────────────────────────────────
            // GUN TURRET: Automated Defensive Twin Cannon
            // ─────────────────────────────────────────────────────────────
            gizmos.circle_2d(pos, size.x * 0.36, Color::srgb(0.22, 0.26, 0.32));
            gizmos.circle_2d(pos, size.x * 0.36, accent_col);

            let angle = turret.barrel_angle;
            let fwd = Vec2::new(angle.cos(), angle.sin());
            let side = Vec2::new(-fwd.y, fwd.x);

            let left_barrel_start = pos + side * 5.0;
            let left_barrel_end = left_barrel_start + fwd * (size.x * 0.65);
            let right_barrel_start = pos - side * 5.0;
            let right_barrel_end = right_barrel_start + fwd * (size.x * 0.65);

            gizmos.line_2d(left_barrel_start, left_barrel_end, Color::srgb(0.92, 0.95, 0.98));
            gizmos.line_2d(right_barrel_start, right_barrel_end, Color::srgb(0.92, 0.95, 0.98));
            gizmos.circle_2d(pos, size.x * 0.18, accent_col);
        }
    }
}

/// Renders gold ore deposits as large, multi-faceted yellowish golden rock boulders with satellite nuggets
fn draw_resources_system(
    mut gizmos: Gizmos,
    fog: Res<FogOfWarGrid>,
    grid_cfg: Option<Res<WorldGridConfig>>,
    query: Query<(&Transform, &ResourceNode)>,
) {
    let default_cfg = WorldGridConfig::default();
    let config = grid_cfg.as_deref().unwrap_or(&default_cfg);

    // Golden Rock Color Palette
    let gold_bright = Color::srgb(1.0, 0.84, 0.18);       // Bright radiant gold edge
    let gold_highlight = Color::srgb(1.0, 0.96, 0.55);    // Lit facet edge / gleam
    let gold_rim = Color::srgba(0.88, 0.68, 0.12, 0.70);  // Inner rim contour
    let gold_shadow = Color::srgba(0.68, 0.46, 0.08, 0.88); // Shadowed facet creases
    let gold_core = Color::srgb(1.0, 1.0, 0.80);         // Sparkling ore fleck
    let gold_aura = Color::srgba(1.0, 0.80, 0.12, 0.22);  // Ambient golden aura

    for (transform, resource) in &query {
        if resource.remaining_minerals == 0 {
            continue;
        }

        let pos = transform.translation.truncate();

        // Shroud gold deposits in unexplored fog
        if fog.get_state_at_world_pos(pos, config) == FogState::Unexplored {
            continue;
        }

        let fullness = (resource.remaining_minerals as f32 / resource.max_minerals as f32).clamp(0.25, 1.0);
        let size = 35.0 * fullness;

        // 1. Soft Ambient Gold Aura
        gizmos.circle_2d(pos, size + 8.0, gold_aura);
        gizmos.circle_2d(pos, size * 0.65, Color::srgba(1.0, 0.85, 0.18, 0.20));

        // 2. Chunky Asymmetric Golden Rock Outline (8-point faceted polygon)
        let rock_pts = [
            pos + Vec2::new(-0.25, 0.98) * size,  // 0: top crest
            pos + Vec2::new(0.45, 1.05) * size,   // 1: top ridge
            pos + Vec2::new(0.98, 0.50) * size,   // 2: upper right cliff
            pos + Vec2::new(1.05, -0.20) * size,  // 3: right edge
            pos + Vec2::new(0.60, -0.88) * size,  // 4: lower right base
            pos + Vec2::new(-0.15, -1.02) * size, // 5: bottom base
            pos + Vec2::new(-0.85, -0.72) * size, // 6: lower left corner
            pos + Vec2::new(-1.02, 0.20) * size,  // 7: left shoulder
        ];

        // Draw primary golden boulder outline
        for i in 0..rock_pts.len() {
            let next = (i + 1) % rock_pts.len();
            gizmos.line_2d(rock_pts[i], rock_pts[next], gold_bright);
        }

        // Inset rim line for chunky rock thickness
        for i in 0..rock_pts.len() {
            let next = (i + 1) % rock_pts.len();
            let p1 = pos + (rock_pts[i] - pos) * 0.88;
            let p2 = pos + (rock_pts[next] - pos) * 0.88;
            gizmos.line_2d(p1, p2, gold_rim);
        }

        // 3. Chiseled 3D Rock Facet Ridges & Creases
        let apex1 = pos + Vec2::new(-0.15, 0.25) * size;
        let apex2 = pos + Vec2::new(0.22, -0.18) * size;

        // Central ridge dividing main rock faces
        gizmos.line_2d(apex1, apex2, gold_highlight);

        // Lit facet ridges radiating from upper crest
        gizmos.line_2d(apex1, rock_pts[0], gold_highlight);
        gizmos.line_2d(apex1, rock_pts[1], gold_bright);
        gizmos.line_2d(apex1, rock_pts[7], gold_bright);

        // Transition ridges
        gizmos.line_2d(apex1, rock_pts[2], gold_bright);
        gizmos.line_2d(apex2, rock_pts[2], gold_bright);

        // Shadowed facet creases radiating from lower ridge
        gizmos.line_2d(apex2, rock_pts[3], gold_shadow);
        gizmos.line_2d(apex2, rock_pts[4], gold_shadow);
        gizmos.line_2d(apex2, rock_pts[5], gold_shadow);
        gizmos.line_2d(apex2, rock_pts[6], gold_shadow);
        gizmos.line_2d(apex1, rock_pts[6], gold_rim);

        // 4. Embedded Gold Ore Flecks & Nuggets on the rock
        gizmos.rect_2d(apex1 + Vec2::new(2.5, 3.0), Vec2::splat(4.5 * fullness), gold_core);
        gizmos.rect_2d(apex2 + Vec2::new(-3.0, -2.0), Vec2::splat(3.5 * fullness), gold_highlight);
        gizmos.circle_2d(pos + Vec2::new(0.35, 0.35) * size, 2.5 * fullness, gold_highlight);
        gizmos.circle_2d(pos + Vec2::new(-0.40, -0.30) * size, 2.0 * fullness, gold_bright);

        // 5. Flanking Satellite Gold Nuggets / Rock Clusters
        let sat1_pos = pos + Vec2::new(-24.0, -13.0) * (0.6 + 0.4 * fullness);
        let sat1_size = 8.0 * fullness;
        let sat1_pts = [
            sat1_pos + Vec2::new(0.0, sat1_size),
            sat1_pos + Vec2::new(sat1_size * 0.9, 0.0),
            sat1_pos + Vec2::new(0.0, -sat1_size),
            sat1_pos + Vec2::new(-sat1_size * 0.9, 0.0),
        ];
        for i in 0..4 {
            gizmos.line_2d(sat1_pts[i], sat1_pts[(i + 1) % 4], gold_bright);
        }
        gizmos.line_2d(sat1_pts[0], sat1_pts[2], gold_highlight);

        let sat2_pos = pos + Vec2::new(23.0, 15.0) * (0.6 + 0.4 * fullness);
        let sat2_size = 9.0 * fullness;
        let sat2_pts = [
            sat2_pos + Vec2::new(0.2, sat2_size),
            sat2_pos + Vec2::new(sat2_size, -0.2 * sat2_size),
            sat2_pos + Vec2::new(-0.2 * sat2_size, -sat2_size),
            sat2_pos + Vec2::new(-sat2_size, 0.1 * sat2_size),
        ];
        for i in 0..4 {
            gizmos.line_2d(sat2_pts[i], sat2_pts[(i + 1) % 4], gold_bright);
        }
        gizmos.line_2d(sat2_pts[0], sat2_pts[2], gold_highlight);

        let sat3_pos = pos + Vec2::new(13.0, -16.0) * (0.6 + 0.4 * fullness);
        gizmos.circle_2d(sat3_pos, 4.0 * fullness, gold_bright);
        gizmos.circle_2d(sat3_pos, 2.0 * fullness, gold_core);
    }
}

/// Renders floating health bars above damaged entities
fn draw_health_bars_system(
    mut gizmos: Gizmos,
    fog: Res<FogOfWarGrid>,
    grid_cfg: Option<Res<WorldGridConfig>>,
    net_client: Res<NetClient>,
    query: Query<(&Transform, &Radius, &Health, &Faction, Option<&Building>)>,
) {
    let default_cfg = WorldGridConfig::default();
    let config = grid_cfg.as_deref().unwrap_or(&default_cfg);

    for (transform, radius, health, faction, building_opt) in &query {
        let is_building = building_opt.is_some();
        let is_damaged = health.current < health.max - 0.5;

        if !is_damaged && !is_building {
            continue;
        }

        let pos = transform.translation.truncate();

        // Shroud health bars of non-friendly entities in fog
        if *faction != net_client.my_faction && *faction != Faction::Neutral
            && fog.get_state_at_world_pos(pos, config) != FogState::Visible {
                continue;
            }

        let bar_w = radius.0 * 2.2;
        let bar_h = 5.0;
        let bar_y = pos.y + radius.0 + 8.0;

        let bar_center = Vec2::new(pos.x, bar_y);
        let bg_col = Color::srgba(0.05, 0.05, 0.08, 0.85);

        let fraction = health.fraction();
        let health_col = if *faction == Faction::Player1 {
            if fraction > 0.5 {
                Color::srgb(0.20, 0.90, 0.35)
            } else if fraction > 0.25 {
                Color::srgb(0.95, 0.80, 0.20)
            } else {
                Color::srgb(0.95, 0.25, 0.25)
            }
        } else {
            Color::srgb(0.95, 0.30, 0.30)
        };

        gizmos.rect_2d(bar_center, Vec2::new(bar_w, bar_h), bg_col);
        let fill_w = bar_w * fraction;
        let fill_center = Vec2::new(pos.x - (bar_w - fill_w) * 0.5, bar_y);
        gizmos.rect_2d(fill_center, Vec2::new(fill_w, bar_h - 1.5), health_col);
    }
}
