use bevy::{
    math::{Quat, Vec3, Vec3Swizzles},
    prelude::{
        Added, Assets, Changed, Commands, Entity, EventWriter, Or, Query, Res, Time, Transform,
        With,
    },
};
use bevy_rapier3d::prelude::{Collider, CollisionGroups, Group, QueryFilter, RapierContext};

use rose_data::ZoneData;
use rose_game_common::messages::client::ClientMessage;

use crate::{
    components::{
        ColliderParent, CollisionHeightOnly, CollisionPlayer, CollisionPlayerGrounding,
        EventObject, NextCommand, Position, WarpObject, COLLISION_FILTER_COLLIDABLE,
        COLLISION_FILTER_GROUND_SUPPORT, COLLISION_GROUP_PHYSICS_TOY,
        COLLISION_GROUP_ZONE_EVENT_OBJECT, COLLISION_GROUP_ZONE_TERRAIN,
        COLLISION_GROUP_ZONE_WARP_OBJECT,
    },
    events::{QuestTriggerEvent, SystemFuncEvent},
    resources::{CurrentZone, GameConnection, GameData},
    zone_loader::ZoneLoaderAsset,
};

const INITIAL_GROUND_SUPPORT_PROBE_HEIGHT: f32 = 20.0;
const PLAYER_JOIN_GROUND_SUPPORT_MAX_RISE_ABOVE_TERRAIN: f32 = 5.0;
const PLAYER_JOIN_REFERENCE_MATCH_DISTANCE: f32 = 200.0;

fn cast_initial_ground_support_height(
    rapier_context: &RapierContext,
    position: &Position,
    terrain_height: f32,
) -> Option<f32> {
    let ray_origin = Vec3::new(
        position.x / 100.0,
        (position.z / 100.0).max(terrain_height) + INITIAL_GROUND_SUPPORT_PROBE_HEIGHT,
        -position.y / 100.0,
    );
    let ray_direction = Vec3::new(0.0, -1.0, 0.0);

    rapier_context
        .cast_ray(
            ray_origin,
            ray_direction,
            INITIAL_GROUND_SUPPORT_PROBE_HEIGHT * 2.0,
            false,
            QueryFilter::new().groups(CollisionGroups::new(
                COLLISION_FILTER_GROUND_SUPPORT,
                !COLLISION_GROUP_PHYSICS_TOY,
            )),
        )
        .map(|(_, distance)| (ray_origin + ray_direction * distance).y)
}

fn cast_join_ground_support_height(
    rapier_context: &RapierContext,
    position: &Position,
    base_height: f32,
) -> Option<f32> {
    let ray_origin = Vec3::new(
        position.x / 100.0,
        base_height + PLAYER_JOIN_GROUND_SUPPORT_MAX_RISE_ABOVE_TERRAIN,
        -position.y / 100.0,
    );
    let ray_direction = Vec3::new(0.0, -1.0, 0.0);

    rapier_context
        .cast_ray(
            ray_origin,
            ray_direction,
            PLAYER_JOIN_GROUND_SUPPORT_MAX_RISE_ABOVE_TERRAIN * 2.0,
            false,
            QueryFilter::new().groups(CollisionGroups::new(
                COLLISION_FILTER_GROUND_SUPPORT,
                !COLLISION_GROUP_PHYSICS_TOY,
            )),
        )
        .map(|(_, distance)| (ray_origin + ray_direction * distance).y)
}

fn get_join_reference_height(zone_data: &ZoneData, position: &Position) -> Option<f32> {
    let target_xy = position.position.xy();
    let mut closest_match = None;

    for candidate in std::iter::once(&zone_data.start_position)
        .chain(zone_data.revive_positions.iter())
        .chain(zone_data.event_positions.values())
    {
        let distance = candidate.xy().distance(target_xy);
        if distance > PLAYER_JOIN_REFERENCE_MATCH_DISTANCE {
            continue;
        }

        if closest_match.map_or(true, |(closest_distance, _)| distance < closest_distance) {
            closest_match = Some((distance, candidate.z / 100.0));
        }
    }

    closest_match.map(|(_, height)| height)
}

fn resolve_player_ground_height(
    zone_data: &ZoneData,
    current_zone_data: &ZoneLoaderAsset,
    rapier_context: &RapierContext,
    position: &Position,
    reference_height: Option<f32>,
) -> f32 {
    let terrain_height = current_zone_data.get_terrain_height(position.x, position.y) / 100.0;
    let probe_base_height = reference_height
        .unwrap_or(terrain_height)
        .max(terrain_height);

    // Prefer supports near the intended spawn anchor when available, otherwise stay close to
    // terrain so ceilings and chandeliers do not override the spawn floor.
    let collision_height =
        cast_join_ground_support_height(rapier_context, position, probe_base_height);

    if let Some(collision_height) = collision_height {
        collision_height.max(terrain_height)
    } else if let Some(reference_height) =
        reference_height.or_else(|| get_join_reference_height(zone_data, position))
    {
        reference_height.max(terrain_height)
    } else {
        terrain_height
    }
}

#[allow(clippy::too_many_arguments)]
pub fn collision_height_only_system(
    mut query_collision_entity: Query<
        (&mut Position, &mut Transform),
        (
            With<CollisionHeightOnly>,
            Or<(Changed<Position>, Changed<Transform>)>,
        ),
    >,
    rapier_context: Res<RapierContext>,
    current_zone: Option<Res<CurrentZone>>,
    zone_loader_assets: Res<Assets<ZoneLoaderAsset>>,
) {
    let current_zone = if let Some(current_zone) = current_zone {
        current_zone
    } else {
        return;
    };
    let current_zone_data =
        if let Some(current_zone_data) = zone_loader_assets.get(&current_zone.handle) {
            current_zone_data
        } else {
            return;
        };

    for (mut position, mut transform) in query_collision_entity.iter_mut() {
        let terrain_height = current_zone_data.get_terrain_height(position.x, position.y) / 100.0;

        // Probe around the intended spawn height so overhead roofs do not win over the floor.
        let collision_height =
            cast_initial_ground_support_height(&rapier_context, &position, terrain_height);

        // Update entity translation and position
        transform.translation.x = position.x / 100.0;
        transform.translation.z = -position.y / 100.0;
        transform.translation.y = if let Some(collision_height) = collision_height {
            collision_height.max(terrain_height)
        } else {
            terrain_height
        };
        position.z = transform.translation.y * 100.0;
    }
}

#[allow(clippy::too_many_arguments)]
pub fn collision_player_system_join_zoin(
    mut commands: Commands,
    mut query_collision_entity: Query<
        (
            Entity,
            &mut Position,
            &mut Transform,
            Option<&CollisionPlayerGrounding>,
        ),
        (
            With<CollisionPlayer>,
            Or<(Changed<CollisionPlayer>, Added<CollisionPlayerGrounding>)>,
        ),
    >,
    game_data: Res<GameData>,
    rapier_context: Res<RapierContext>,
    current_zone: Option<Res<CurrentZone>>,
    zone_loader_assets: Res<Assets<ZoneLoaderAsset>>,
) {
    let current_zone = if let Some(current_zone) = current_zone {
        current_zone
    } else {
        return;
    };
    let current_zone_data =
        if let Some(current_zone_data) = zone_loader_assets.get(&current_zone.handle) {
            current_zone_data
        } else {
            return;
        };

    let zone_data = game_data
        .zones
        .get_zone(current_zone.id)
        .expect("current zone data should be loaded");

    for (entity, mut position, mut transform, revive_grounding) in query_collision_entity.iter_mut()
    {
        let reference_height = if revive_grounding.is_some() {
            Some(position.z / 100.0)
        } else {
            game_data
                .zones
                .get_zone(current_zone.id)
                .and_then(|zone_data| get_join_reference_height(zone_data, &position))
        };
        let target_y = resolve_player_ground_height(
            zone_data,
            current_zone_data,
            &rapier_context,
            &position,
            reference_height,
        );

        // Update entity translation and position
        transform.translation.x = position.x / 100.0;
        transform.translation.z = -position.y / 100.0;
        transform.translation.y = target_y;
        position.z = transform.translation.y * 100.0;

        if revive_grounding.is_some() {
            commands.entity(entity).remove::<CollisionPlayerGrounding>();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use bevy::math::{Vec2, Vec3};

    use rose_data::{ZoneData, ZoneId};

    use crate::components::Position;

    use super::get_join_reference_height;

    fn create_zone_data() -> ZoneData {
        let mut event_positions = HashMap::new();
        event_positions.insert("entry".to_string(), Vec3::new(3000.0, 4000.0, 1500.0));

        ZoneData {
            id: ZoneId::new(1).unwrap(),
            name: "Test Zone",
            description: "",
            pvp_state: 0,
            join_trigger: None,
            kill_trigger: None,
            dead_trigger: None,
            sector_size: 0,
            grid_per_patch: 0.0,
            grid_size: 0.0,
            event_objects: Vec::new(),
            monster_spawns: Vec::new(),
            npcs: Vec::new(),
            sectors_base_position: Vec2::ZERO,
            num_sectors_x: 0,
            num_sectors_y: 0,
            start_position: Vec3::new(1000.0, 2000.0, 900.0),
            revive_positions: vec![Vec3::new(2000.0, 3000.0, 1200.0)],
            event_positions,
            day_cycle: 0,
            morning_time: 0,
            day_time: 0,
            evening_time: 0,
            night_time: 0,
            skybox_id: None,
            party_xp_a: 0,
            party_xp_b: 0,
        }
    }

    #[test]
    fn join_reference_matches_start_position_height() {
        let zone_data = create_zone_data();
        let position = Position::new(Vec3::new(1000.0, 2000.0, 0.0));

        assert_eq!(get_join_reference_height(&zone_data, &position), Some(9.0));
    }

    #[test]
    fn join_reference_matches_event_position_height() {
        let zone_data = create_zone_data();
        let position = Position::new(Vec3::new(3000.0, 4000.0, 0.0));

        assert_eq!(get_join_reference_height(&zone_data, &position), Some(15.0));
    }

    #[test]
    fn join_reference_ignores_distant_positions() {
        let zone_data = create_zone_data();
        let position = Position::new(Vec3::new(5000.0, 6000.0, 0.0));

        assert_eq!(get_join_reference_height(&zone_data, &position), None);
    }
}

#[allow(clippy::too_many_arguments)]
pub fn collision_player_system(
    mut commands: Commands,
    mut query_collision_entity: Query<
        (Entity, &mut Position, &mut Transform),
        With<CollisionPlayer>,
    >,
    mut query_event_object: Query<&mut EventObject>,
    mut quest_trigger_events: EventWriter<QuestTriggerEvent>,
    mut system_func_events: EventWriter<SystemFuncEvent>,
    mut query_warp_object: Query<&mut WarpObject>,
    query_collider_parent: Query<&ColliderParent>,
    current_zone: Option<Res<CurrentZone>>,
    game_connection: Option<Res<GameConnection>>,
    rapier_context: Res<RapierContext>,
    time: Res<Time>,
    zone_loader_assets: Res<Assets<ZoneLoaderAsset>>,
) {
    let current_zone = if let Some(current_zone) = current_zone {
        current_zone
    } else {
        return;
    };
    let current_zone_data =
        if let Some(current_zone_data) = zone_loader_assets.get(&current_zone.handle) {
            current_zone_data
        } else {
            return;
        };

    for (entity, mut position, mut transform) in query_collision_entity.iter_mut() {
        // Cast ray forward to collide with walls
        let new_translation = Vec3::new(
            position.x / 100.0,
            transform.translation.y,
            -position.y / 100.0,
        );
        let collider_radius = 0.4;
        let translation_delta = new_translation - transform.translation;
        if translation_delta.length() > 0.00001 {
            let cast_origin = transform.translation + Vec3::new(0.0, 1.2, 0.0);
            let cast_direction = translation_delta.normalize();

            if let Some((_, distance)) = rapier_context.cast_shape(
                cast_origin + cast_direction * collider_radius,
                Quat::default(),
                cast_direction,
                &Collider::ball(collider_radius),
                translation_delta.length(),
                QueryFilter::new().groups(CollisionGroups::new(
                    COLLISION_FILTER_COLLIDABLE,
                    !COLLISION_GROUP_ZONE_TERRAIN & !COLLISION_GROUP_PHYSICS_TOY,
                )),
            ) {
                let collision_translation =
                    cast_origin + translation_delta * (distance.toi - 0.1).max(0.0);
                position.x = collision_translation.x * 100.0;
                position.y = -(collision_translation.z * 100.0);
                position.z = collision_translation.y * 100.0;

                commands.entity(entity).insert(NextCommand::with_stop());

                if let Some(game_connection) = game_connection.as_ref() {
                    game_connection
                        .client_message_tx
                        .send(ClientMessage::MoveCollision {
                            position: position.position,
                        })
                        .ok();
                }
            }
        }

        // Cast ray down to see if we are standing on any objects
        let fall_distance = time.delta_seconds() * 9.81;
        let ray_origin = Vec3::new(
            position.x / 100.0,
            position.z / 100.0 + 1.35,
            -position.y / 100.0,
        );
        let ray_direction = Vec3::new(0.0, -1.0, 0.0);
        let collision_height = if let Some((_, distance)) = rapier_context.cast_ray(
            ray_origin,
            ray_direction,
            1.35 + fall_distance,
            false,
            QueryFilter::new().groups(CollisionGroups::new(
                COLLISION_FILTER_GROUND_SUPPORT,
                !COLLISION_GROUP_PHYSICS_TOY,
            )),
        ) {
            Some((ray_origin + ray_direction * distance).y)
        } else {
            None
        };

        // We can never be below the heightmap
        let terrain_height = current_zone_data.get_terrain_height(position.x, position.y) / 100.0;

        let target_y = if let Some(collision_height) = collision_height {
            collision_height.max(terrain_height)
        } else {
            terrain_height
        };

        // Update entity translation and position
        transform.translation.x = position.x / 100.0;
        transform.translation.z = -position.y / 100.0;

        if transform.translation.y - target_y > fall_distance {
            transform.translation.y -= fall_distance;
        } else {
            transform.translation.y = target_y;
        }

        position.z = transform.translation.y * 100.0;

        // Check if we are now colliding with any warp / event object
        rapier_context.intersections_with_shape(
            Vec3::new(
                position.x / 100.0,
                position.z / 100.0 + 1.0,
                -position.y / 100.0,
            ),
            Quat::default(),
            &Collider::ball(1.0),
            QueryFilter::new().groups(CollisionGroups::new(
                Group::all(),
                COLLISION_GROUP_ZONE_EVENT_OBJECT | COLLISION_GROUP_ZONE_WARP_OBJECT,
            )),
            |hit_entity| {
                let hit_entity = query_collider_parent
                    .get(hit_entity)
                    .map_or(hit_entity, |collider_parent| collider_parent.entity);

                if let Ok(mut hit_event_object) = query_event_object.get_mut(hit_entity) {
                    if time.elapsed_seconds_f64() - hit_event_object.last_collision > 5.0 {
                        if !hit_event_object.quest_trigger_name.is_empty()
                            && !hit_event_object
                                .quest_trigger_name
                                .eq_ignore_ascii_case("EMPTY")
                        {
                            quest_trigger_events.send(QuestTriggerEvent::DoTrigger(
                                hit_event_object.quest_trigger_name.as_str().into(),
                            ));
                        }

                        if !hit_event_object.script_function_name.is_empty()
                            && !hit_event_object
                                .script_function_name
                                .eq_ignore_ascii_case("EMPTY")
                        {
                            system_func_events.send(SystemFuncEvent::CallFunction(
                                hit_event_object.script_function_name.clone(),
                                vec![],
                            ));
                        }

                        hit_event_object.last_collision = time.elapsed_seconds_f64();
                    }
                } else if let Ok(mut hit_warp_object) = query_warp_object.get_mut(hit_entity) {
                    if time.elapsed_seconds_f64() - hit_warp_object.last_collision > 5.0 {
                        if let Some(game_connection) = game_connection.as_ref() {
                            game_connection
                                .client_message_tx
                                .send(ClientMessage::WarpGateRequest {
                                    warp_gate_id: hit_warp_object.warp_id,
                                })
                                .ok();
                        }

                        hit_warp_object.last_collision = time.elapsed_seconds_f64();
                    }
                }
                true
            },
        );
    }
}
