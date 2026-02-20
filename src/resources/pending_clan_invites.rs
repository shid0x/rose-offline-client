use bevy::prelude::Resource;
use rose_game_common::components::{ClanLevel, ClanUniqueId};

pub struct PendingClanInvite {
    pub inviter_name: String,
    pub clan_name: String,
    pub clan_unique_id: ClanUniqueId,
    pub clan_level: ClanLevel,
}

#[derive(Default, Resource)]
pub struct PendingClanInvites {
    pub invites: Vec<PendingClanInvite>,
}
