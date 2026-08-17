use bevy::prelude::*;
use shared::components::{
    Barracks, BaseHQ, Building, Faction, GunTurret, Health, Radius, ResourceNode, SiegeTank, Soldier,
    SupplyDepot, Unit, Worker,
};

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

/// Renders units (Workers, Soldiers, Siege Tanks) with faction colors, heading indicators, and weapons
fn draw_units_system(
    mut gizmos: Gizmos,
    query: Query<(
        &Transform,
        &Radius,
        &Faction,
        Option<&Worker>,
        Option<&Soldier>,
        Option<&SiegeTank>,
    ), With<Unit>>,
) {
    for (transform, radius, faction, worker_opt, soldier_opt, tank_opt) in &query {
        let pos = transform.translation.truncate();
        let r = radius.0;
        let rot = transform.rotation.to_euler(EulerRot::ZYX).0;

        let [cr, cg, cb, _] = faction.color_rgba();
        let body_color = Color::srgb(cr, cg, cb);
        let outline_color = body_color.lighter(0.25);

        if let Some(tank) = tank_opt {
            // ─────────────────────────────────────────────────────────────
            // SIEGE TANK: Heavy Armored Tracked Combat Vehicle
            // ─────────────────────────────────────────────────────────────
            let forward = Vec2::new(rot.cos(), rot.sin());
            let side = Vec2::new(-forward.y, forward.x);

            // Left and Right Caterpillar Treads
            let tread_w = r * 0.45;
            let tread_l = r * 1.8;
            let left_tread = pos + side * (r * 0.7);
            let right_tread = pos - side * (r * 0.7);
            let tread_col = Color::srgb(0.20, 0.22, 0.25);

            gizmos.rect_2d(left_tread, Vec2::new(tread_l, tread_w), tread_col);
            gizmos.rect_2d(right_tread, Vec2::new(tread_l, tread_w), tread_col);

            // Heavy Armored Hull Chassis
            gizmos.rect_2d(pos, Vec2::new(r * 1.5, r * 1.2), body_color);
            gizmos.rect_2d(pos, Vec2::new(r * 1.5, r * 1.2), outline_color);

            // Rotating Artillery Turret Box
            let t_angle = tank.turret_angle;
            let t_fwd = Vec2::new(t_angle.cos(), t_angle.sin());

            gizmos.circle_2d(pos, r * 0.55, Color::srgb(0.15, 0.18, 0.22));

            gizmos.circle_2d(pos, r * 0.55, outline_color);

            // Long Artillery Cannon Barrel with Muzzle Brake
            let barrel_base = pos + t_fwd * (r * 0.3);
            let barrel_tip = pos + t_fwd * (r * 1.6);
            gizmos.line_2d(barrel_base, barrel_tip, Color::srgb(0.92, 0.95, 0.98));
            gizmos.rect_2d(barrel_tip, Vec2::new(5.0, 7.0), Color::srgb(0.35, 0.40, 0.45));
        } else {
            // Body Circle for infantry/worker
            gizmos.circle_2d(pos, r, body_color);
            gizmos.circle_2d(pos, r, outline_color);

            // Heading Direction Pointer
            let forward = Vec2::new(rot.cos(), rot.sin());
            let tip = pos + forward * (r + 6.0);
            let left = pos + forward * (r - 2.0) + Vec2::new(-forward.y, forward.x) * 4.0;
            let right = pos + forward * (r - 2.0) - Vec2::new(-forward.y, forward.x) * 4.0;

            gizmos.line_2d(tip, left, Color::WHITE);
            gizmos.line_2d(tip, right, Color::WHITE);
            gizmos.line_2d(left, right, Color::WHITE);

            // Marine Rifle Barrel
            if soldier_opt.is_some() {
                let gun_tip = pos + forward * (r + 10.0);
                let gun_base = pos + forward * (r + 2.0);
                gizmos.line_2d(gun_base, gun_tip, Color::srgb(0.9, 0.9, 0.95));
            }

            // SCV Welder Arms
            if worker_opt.is_some() {
                let arm_left = pos + forward * (r + 4.0) + Vec2::new(-forward.y, forward.x) * 5.0;
                let arm_right = pos + forward * (r + 4.0) - Vec2::new(-forward.y, forward.x) * 5.0;
                gizmos.circle_2d(arm_left, 2.5, Color::srgb(0.95, 0.75, 0.20));
                gizmos.circle_2d(arm_right, 2.5, Color::srgb(0.95, 0.75, 0.20));
            }
        }
    }
}

/// Renders buildings with distinct architectural silhouettes based on building type
fn draw_buildings_system(
    mut gizmos: Gizmos,
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
    for (transform, building, faction, hq_opt, barracks_opt, supply_opt, turret_opt) in &query {
        let pos = transform.translation.truncate();
        let size = building.size;

        let [cr, cg, cb, _] = faction.color_rgba();
        let accent_col = Color::srgb(cr, cg, cb);
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

/// Renders mineral crystal fields with crystalline facets and remaining node clusters
fn draw_resources_system(mut gizmos: Gizmos, query: Query<(&Transform, &ResourceNode)>) {
    let crystal_col = Color::srgb(0.22, 0.90, 1.0);
    let crystal_glow = Color::srgba(0.22, 0.90, 1.0, 0.35);

    for (transform, resource) in &query {
        if resource.remaining_minerals == 0 {
            continue;
        }

        let pos = transform.translation.truncate();
        let fullness = (resource.remaining_minerals as f32 / resource.max_minerals as f32).clamp(0.2, 1.0);
        let size = 26.0 * fullness;

        // Central large crystal
        let p_top = pos + Vec2::new(0.0, size);
        let p_right = pos + Vec2::new(size * 0.8, 0.0);
        let p_bottom = pos + Vec2::new(0.0, -size);
        let p_left = pos + Vec2::new(-size * 0.8, 0.0);

        gizmos.line_2d(p_top, p_right, crystal_col);
        gizmos.line_2d(p_right, p_bottom, crystal_col);
        gizmos.line_2d(p_bottom, p_left, crystal_col);
        gizmos.line_2d(p_left, p_top, crystal_col);
        gizmos.line_2d(p_left, p_right, Color::WHITE);
        gizmos.line_2d(p_top, p_bottom, Color::srgba(1.0, 1.0, 1.0, 0.5));

        // Flanking small crystal clusters
        let left_cluster = pos + Vec2::new(-16.0, -6.0);
        let right_cluster = pos + Vec2::new(16.0, 8.0);
        gizmos.circle_2d(left_cluster, 6.0 * fullness, crystal_col);
        gizmos.circle_2d(right_cluster, 7.0 * fullness, crystal_col);

        gizmos.circle_2d(pos, size * 0.5, crystal_glow);
    }
}

/// Renders floating health bars above damaged entities
fn draw_health_bars_system(
    mut gizmos: Gizmos,
    query: Query<(&Transform, &Radius, &Health, &Faction, Option<&Building>)>,
) {
    for (transform, radius, health, faction, building_opt) in &query {
        let is_building = building_opt.is_some();
        let is_damaged = health.current < health.max - 0.5;

        if !is_damaged && !is_building {
            continue;
        }

        let pos = transform.translation.truncate();
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
