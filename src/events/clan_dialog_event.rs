use bevy::prelude::Event;
use rose_game_common::components::{ClanLevel, ClanUniqueId};
use rose_game_common::messages::ClientEntityId;

#[derive(Event)]
pub enum ClanDialogEvent {
    Open {
        npc_entity_id: Option<ClientEntityId>,
    },
    InviteReceived {
        inviter_name: String,
        clan_unique_id: ClanUniqueId,
        clan_name: String,
        clan_level: ClanLevel,
    },
}
