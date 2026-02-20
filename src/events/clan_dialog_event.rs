use bevy::prelude::Event;
use rose_game_common::components::{ClanLevel, ClanUniqueId};

#[derive(Event)]
pub enum ClanDialogEvent {
    Open,
    InviteReceived {
        inviter_name: String,
        clan_unique_id: ClanUniqueId,
        clan_name: String,
        clan_level: ClanLevel,
    },
}
