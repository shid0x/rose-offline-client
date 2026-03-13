use bevy::prelude::Component;

use rose_game_common::{
    components::CharacterUniqueId,
    messages::{server::PartyMemberInfo, ClientEntityId, PartyItemSharing, PartyXpSharing},
};

pub enum PartyOwner {
    Unknown,
    Player,
    Character(CharacterUniqueId),
}

#[derive(Component)]
pub struct PartyInfo {
    pub owner: PartyOwner,
    pub members: Vec<PartyMemberInfo>,
    pub item_sharing: PartyItemSharing,
    pub xp_sharing: PartyXpSharing,
    pub level: i32,
    pub experience: i32,
}

impl Default for PartyInfo {
    fn default() -> Self {
        Self {
            owner: PartyOwner::Unknown,
            members: Vec::new(),
            item_sharing: PartyItemSharing::EqualLootDistribution,
            xp_sharing: PartyXpSharing::EqualShare,
            level: 1,
            experience: 0,
        }
    }
}

impl PartyInfo {
    pub fn contains_member(&self, client_entity_id: ClientEntityId) -> bool {
        self.members
            .iter()
            .any(|member| member.get_client_entity_id() == Some(client_entity_id))
    }
}
