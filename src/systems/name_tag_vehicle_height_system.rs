use bevy::prelude::{Added, Entity, Query, RemovedComponents, Res, Transform, With, Without};

use rose_game_common::components::Equipment;

use crate::{
    components::{ModelHeight, NameTag, NameTagEntity, Vehicle},
    model_loader::ModelLoader,
};

pub fn name_tag_vehicle_height_system(
    query_new_drivers: Query<(&NameTagEntity, &Equipment, Option<&ModelHeight>), Added<Vehicle>>,
    query_stopped_drivers: Query<(&NameTagEntity, &ModelHeight), Without<Vehicle>>,
    mut removed_vehicles: RemovedComponents<Vehicle>,
    mut query_name_tag_transform: Query<&mut Transform, With<NameTag>>,
    model_loader: Res<ModelLoader>,
) {
    // When a vehicle is added, update the name tag height to account for the vehicle
    // The name should appear at: driver seat height + character's own model height
    for (name_tag_entity, equipment, model_height) in query_new_drivers.iter() {
        let driver_seat_height = model_loader.get_vehicle_driver_seat_height(equipment);
        let character_height = model_height.map_or(1.8, |h| h.height);
        if let Ok(mut transform) = query_name_tag_transform.get_mut(name_tag_entity.0) {
            transform.translation.y = driver_seat_height + character_height;
        }
    }

    // When a vehicle is removed, restore the name tag height to the character's model height
    for entity in removed_vehicles.iter() {
        if let Ok((name_tag_entity, model_height)) = query_stopped_drivers.get(entity) {
            if let Ok(mut transform) = query_name_tag_transform.get_mut(name_tag_entity.0) {
                transform.translation.y = model_height.height;
            }
        }
    }
}
