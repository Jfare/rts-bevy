use super::*;
use crate::session::{Matchmaker, Room};
use shared::components::*;
use shared::protocol::GameMode;

fn mining_app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.insert_resource(Time::<()>::default());
    let mut mm = Matchmaker::new();
    let mut room = Room::new(1, None, GameMode::Multiplayer1v1, Some(101), Some(102));
    room.is_active = true;
    room.countdown_timer = 0.0;
    mm.rooms.insert(1, room);
    app.insert_resource(mm);
    app.add_systems(Update, server_mining_system);

    // 110x110 Base HQ at the origin
    app.world_mut().spawn((
        Transform::from_xyz(0.0, 0.0, 1.0),
        Faction::Player1,
        RoomId(1),
        BaseHQ { supply_provided: 10, dropoff_radius: 70.0 },
    ));
    app
}

fn spawn_returning_worker(app: &mut App, pos: Vec2) -> Entity {
    let mut worker = Worker::default();
    worker.state = WorkerState::MovingToBase;
    worker.carried_minerals = 5;
    app.world_mut()
        .spawn((
            Transform::from_xyz(pos.x, pos.y, 2.0),
            MoveSpeed(WORKER_MOVE_SPEED),
            Faction::Player1,
            RoomId(1),
            worker,
        ))
        .id()
}

fn p1_minerals(app: &App) -> u32 {
    app.world()
        .resource::<Matchmaker>()
        .rooms
        .get(&1)
        .unwrap()
        .economy
        .get_minerals(Faction::Player1)
}

#[test]
fn test_worker_drops_off_at_hq_corner() {
    let mut app = mining_app();
    // ~14px from the HQ corner but ~92px from its centre (old 72px centre check failed here)
    let worker_ent = spawn_returning_worker(&mut app, Vec2::new(65.0, 65.0));
    let before = p1_minerals(&app);

    app.update();

    let worker = app.world().get::<Worker>(worker_ent).unwrap();
    assert_eq!(worker.carried_minerals, 0, "Worker at HQ corner must deposit minerals");
    assert_eq!(p1_minerals(&app), before + 5);
}

#[test]
fn test_worker_far_from_hq_edge_does_not_drop_off() {
    let mut app = mining_app();
    // ~63px from the corner: too far
    let worker_ent = spawn_returning_worker(&mut app, Vec2::new(100.0, 100.0));
    let before = p1_minerals(&app);

    app.update();

    let worker = app.world().get::<Worker>(worker_ent).unwrap();
    assert_eq!(worker.carried_minerals, 5);
    assert_eq!(p1_minerals(&app), before);
}
