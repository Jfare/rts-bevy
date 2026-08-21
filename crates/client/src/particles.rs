use bevy::prelude::*;
use shared::components::{Building, Faction, Health};

#[derive(Component)]
pub struct Particle {
    pub velocity: Vec2,
    pub drag: f32,
    pub lifetime: f32,
    pub max_lifetime: f32,
    pub start_size: f32,
    pub end_size: f32,
    pub start_color: Color,
    pub end_color: Color,
}

#[derive(Component)]
pub struct Shockwave {
    pub lifetime: f32,
    pub max_lifetime: f32,
    pub max_radius: f32,
    pub color: Color,
}

#[derive(Event, Clone, Copy, Debug)]
pub enum ParticleEvent {
    Explosion { pos: Vec2, is_heavy: bool },
    Sparks { pos: Vec2, dir: Vec2, count: usize },
    StimpackVapor { pos: Vec2 },
    MuzzleSmoke { pos: Vec2, dir: Vec2 },
    Shockwave { pos: Vec2, radius: f32, color: Color },
}

pub struct ParticlesPlugin;

impl Plugin for ParticlesPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<ParticleEvent>()
            .add_systems(
                Update,
                (
                    handle_particle_events,
                    update_particles_system,
                    structure_damage_smoke_system,
                    draw_particles_system,
                ),
            );
    }
}

/// Spawns particle entities from incoming ParticleEvent triggers
fn handle_particle_events(
    mut commands: Commands,
    mut events: EventReader<ParticleEvent>,
) {
    for event in events.read() {
        match *event {
            ParticleEvent::Explosion { pos, is_heavy } => {
                let spark_count = if is_heavy { 32 } else { 18 };
                let max_speed = if is_heavy { 240.0 } else { 160.0 };

                // 1. Fiery debris sparks
                for i in 0..spark_count {
                    let angle = (i as f32 / spark_count as f32) * std::f32::consts::TAU + (i as f32 * 1.37);
                    let speed = 40.0 + (i as f32 % 5.0) * (max_speed / 4.0);
                    let vel = Vec2::new(angle.cos(), angle.sin()) * speed;
                    let life = if is_heavy { 0.55 + (i as f32 % 3.0) * 0.15 } else { 0.35 + (i as f32 % 3.0) * 0.10 };

                    commands.spawn((
                        Particle {
                            velocity: vel,
                            drag: 0.88,
                            lifetime: 0.0,
                            max_lifetime: life,
                            start_size: if is_heavy { 6.0 } else { 4.0 },
                            end_size: 1.0,
                            start_color: if i % 2 == 0 { Color::srgb(1.0, 0.75, 0.2) } else { Color::srgb(1.0, 0.3, 0.05) },
                            end_color: Color::srgba(0.8, 0.1, 0.0, 0.0),
                        },
                        Transform::from_xyz(pos.x, pos.y, 4.0),
                    ));
                }

                // 2. Lingering dark smoke puffs
                let smoke_count = if is_heavy { 8 } else { 4 };
                for i in 0..smoke_count {
                    let angle = (i as f32 / smoke_count as f32) * std::f32::consts::TAU;
                    let vel = Vec2::new(angle.cos(), angle.sin()) * 25.0;
                    commands.spawn((
                        Particle {
                            velocity: vel,
                            drag: 0.95,
                            lifetime: 0.0,
                            max_lifetime: 0.75,
                            start_size: 5.0,
                            end_size: 18.0,
                            start_color: Color::srgba(0.25, 0.25, 0.28, 0.65),
                            end_color: Color::srgba(0.12, 0.12, 0.14, 0.0),
                        },
                        Transform::from_xyz(pos.x, pos.y, 3.8),
                    ));
                }

                // 3. Expanding blast shockwave
                commands.spawn((
                    Shockwave {
                        lifetime: 0.0,
                        max_lifetime: if is_heavy { 0.40 } else { 0.25 },
                        max_radius: if is_heavy { 65.0 } else { 35.0 },
                        color: if is_heavy { Color::srgba(1.0, 0.6, 0.2, 0.85) } else { Color::srgba(1.0, 0.85, 0.4, 0.75) },
                    },
                    Transform::from_xyz(pos.x, pos.y, 3.7),
                ));
            }
            ParticleEvent::Sparks { pos, dir, count } => {
                for i in 0..count {
                    let spread = ((i as f32) - (count as f32 / 2.0)) * 0.35;
                    let rotated_dir = Vec2::new(
                        dir.x * spread.cos() - dir.y * spread.sin(),
                        dir.x * spread.sin() + dir.y * spread.cos(),
                    );
                    let speed = 90.0 + (i as f32 * 25.0);
                    commands.spawn((
                        Particle {
                            velocity: -rotated_dir * speed,
                            drag: 0.85,
                            lifetime: 0.0,
                            max_lifetime: 0.22,
                            start_size: 3.5,
                            end_size: 0.8,
                            start_color: Color::srgb(1.0, 0.95, 0.5),
                            end_color: Color::srgba(1.0, 0.3, 0.1, 0.0),
                        },
                        Transform::from_xyz(pos.x, pos.y, 4.0),
                    ));
                }
            }
            ParticleEvent::StimpackVapor { pos } => {
                for i in 0..12 {
                    let angle = (i as f32 / 12.0) * std::f32::consts::TAU;
                    let speed = 35.0 + (i as f32 % 3.0) * 15.0;
                    commands.spawn((
                        Particle {
                            velocity: Vec2::new(angle.cos() * speed, angle.sin() * speed + 20.0),
                            drag: 0.92,
                            lifetime: 0.0,
                            max_lifetime: 0.45,
                            start_size: 4.0,
                            end_size: 9.0,
                            start_color: Color::srgba(0.2, 0.95, 0.95, 0.85),
                            end_color: Color::srgba(0.1, 0.5, 0.9, 0.0),
                        },
                        Transform::from_xyz(pos.x, pos.y, 3.8),
                    ));
                }
            }
            ParticleEvent::MuzzleSmoke { pos, dir } => {
                for i in 0..3 {
                    let speed = 20.0 + (i as f32 * 10.0);
                    commands.spawn((
                        Particle {
                            velocity: dir * speed + Vec2::new((i as f32 - 1.0) * 8.0, 5.0),
                            drag: 0.90,
                            lifetime: 0.0,
                            max_lifetime: 0.30,
                            start_size: 3.0,
                            end_size: 8.0,
                            start_color: Color::srgba(0.7, 0.7, 0.75, 0.45),
                            end_color: Color::srgba(0.3, 0.3, 0.35, 0.0),
                        },
                        Transform::from_xyz(pos.x, pos.y, 3.8),
                    ));
                }
            }
            ParticleEvent::Shockwave { pos, radius, color } => {
                commands.spawn((
                    Shockwave {
                        lifetime: 0.0,
                        max_lifetime: 0.35,
                        max_radius: radius,
                        color,
                    },
                    Transform::from_xyz(pos.x, pos.y, 3.7),
                ));
            }
        }
    }
}

/// Updates positions, lifespans, and despawns completed particles & shockwaves
fn update_particles_system(
    mut commands: Commands,
    time: Res<Time>,
    mut particle_query: Query<(Entity, &mut Particle, &mut Transform)>,
    mut shockwave_query: Query<(Entity, &mut Shockwave)>,
) {
    let dt = time.delta_secs();

    for (entity, mut particle, mut transform) in &mut particle_query {
        particle.lifetime += dt;
        if particle.lifetime >= particle.max_lifetime {
            commands.entity(entity).despawn();
            continue;
        }

        let drag_factor = particle.drag.powf(dt * 60.0);
        particle.velocity *= drag_factor;
        transform.translation.x += particle.velocity.x * dt;
        transform.translation.y += particle.velocity.y * dt;
    }

    for (entity, mut shockwave) in &mut shockwave_query {
        shockwave.lifetime += dt;
        if shockwave.lifetime >= shockwave.max_lifetime {
            commands.entity(entity).despawn();
        }
    }
}

/// Periodically spawns smoke and electrical sparks on damaged buildings
fn structure_damage_smoke_system(
    mut particle_events: EventWriter<ParticleEvent>,
    time: Res<Time>,
    mut timer: Local<f32>,
    building_query: Query<(&Transform, &Health, &Faction), With<Building>>,
) {
    *timer += time.delta_secs();
    if *timer < 0.25 {
        return;
    }
    *timer = 0.0;

    for (transform, health, _) in &building_query {
        if health.is_dead() {
            continue;
        }
        let ratio = health.current / health.max;
        if ratio < 0.50 {
            let pos = transform.translation.truncate();
            let offset = Vec2::new(
                (rand_pseudo(pos.x + *timer) * 30.0) - 15.0,
                rand_pseudo(pos.y + *timer) * 20.0,
            );
            particle_events.send(ParticleEvent::MuzzleSmoke {
                pos: pos + offset,
                dir: Vec2::new(0.0, 1.0),
            });

            if ratio < 0.25 {
                particle_events.send(ParticleEvent::Sparks {
                    pos: pos + offset,
                    dir: Vec2::new(0.0, 1.0),
                    count: 3,
                });
            }
        }
    }
}

fn rand_pseudo(seed: f32) -> f32 {
    let x = (seed.sin() * 43758.5453).fract();
    x.abs()
}

/// Draws active particles and expanding shockwaves with Gizmos
fn draw_particles_system(
    mut gizmos: Gizmos,
    particles: Query<(&Transform, &Particle)>,
    shockwaves: Query<(&Transform, &Shockwave)>,
) {
    for (transform, p) in &particles {
        let t = (p.lifetime / p.max_lifetime).clamp(0.0, 1.0);
        let size = p.start_size + (p.end_size - p.start_size) * t;
        let color = lerp_color(p.start_color, p.end_color, t);
        gizmos.circle_2d(transform.translation.truncate(), size, color);
    }

    for (transform, s) in &shockwaves {
        let t = (s.lifetime / s.max_lifetime).clamp(0.0, 1.0);
        let radius = s.max_radius * (1.0 - (1.0 - t).powi(2));
        let alpha_fade = (1.0 - t).max(0.0);
        let mut color = s.color;
        let [r, g, b, a] = color.to_srgba().to_f32_array();
        color = Color::srgba(r, g, b, a * alpha_fade);
        gizmos.circle_2d(transform.translation.truncate(), radius, color);
    }
}

fn lerp_color(a: Color, b: Color, t: f32) -> Color {
    let [ar, ag, ab, aa] = a.to_srgba().to_f32_array();
    let [br, bg, bb, ba] = b.to_srgba().to_f32_array();
    Color::srgba(
        ar + (br - ar) * t,
        ag + (bg - ag) * t,
        ab + (bb - ab) * t,
        aa + (ba - aa) * t,
    )
}
