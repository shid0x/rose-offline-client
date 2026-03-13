use bevy::prelude::Event;

use rose_game_common::messages::ClientEntityId;

#[derive(Event)]
pub enum CraftEvent {
    UpgradeCompleted,
    OpenNpcDisassemble { client_entity_id: ClientEntityId },
    OpenNpcUpgrade { client_entity_id: ClientEntityId },
}
