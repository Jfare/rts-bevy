pub mod combat;
pub mod economy;
pub mod movement;
pub mod social;

use bevy::prelude::*;
use shared::components::*;
use crate::session::Matchmaker;

pub type UnitQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Transform,
        &'static NetEntity,
        &'static Faction,
        &'static RoomId,
        Option<&'static mut MoveTarget>,
        Option<&'static mut Soldier>,
        Option<&'static mut Worker>,
        Option<&'static mut MeleeFighter>,
        Option<&'static mut Health>,
        Option<&'static mut TacticalStance>,
    ),
    Without<ResourceNode>,
>;

pub type NodeQuery<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static NetEntity, &'static Transform, &'static RoomId),
    With<ResourceNode>,
>;

pub type ProdQuery<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static NetEntity, &'static Faction, &'static RoomId, &'static mut ProductionBuilding),
>;

/// Helper to look up a peer's assigned faction and active room ID
#[inline]
pub fn get_player_and_room(matchmaker: &Matchmaker, peer_id: u64) -> (Faction, u32) {
    let faction = matchmaker
        .players
        .get(&peer_id)
        .map(|p| p.faction)
        .unwrap_or(Faction::Player1);
    let room = matchmaker
        .players
        .get(&peer_id)
        .map(|p| p.room_id)
        .unwrap_or(0);
    (faction, room)
}

