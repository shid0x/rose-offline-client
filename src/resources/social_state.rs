use std::collections::HashMap;

use bevy::prelude::Resource;
use rose_game_common::{
    components::CharacterUniqueId,
    messages::{FriendInfo, FriendStatus},
};

pub struct PendingFriendRequest {
    pub requester_id: CharacterUniqueId,
    pub name: String,
}

#[derive(Clone)]
pub struct PrivateChatEntry {
    pub from_name: String,
    pub text: String,
    pub is_local: bool,
}

#[derive(Default, Resource)]
pub struct SocialState {
    pub friends: Vec<FriendInfo>,
    pub pending_requests: Vec<PendingFriendRequest>,
    pub chat_histories: HashMap<CharacterUniqueId, Vec<PrivateChatEntry>>,
    pub open_chat_requests: Vec<CharacterUniqueId>,
}

impl SocialState {
    pub fn clear(&mut self) {
        self.friends.clear();
        self.pending_requests.clear();
        self.chat_histories.clear();
        self.open_chat_requests.clear();
    }

    pub fn upsert_friend(&mut self, friend: FriendInfo) {
        if let Some(existing) = self
            .friends
            .iter_mut()
            .find(|existing| existing.character_id == friend.character_id)
        {
            *existing = friend;
        } else {
            self.friends.push(friend);
        }
    }

    pub fn remove_friend(&mut self, friend_id: CharacterUniqueId) {
        self.friends
            .retain(|friend| friend.character_id != friend_id);
    }

    pub fn update_friend_status(
        &mut self,
        friend_id: CharacterUniqueId,
        status: FriendStatus,
    ) -> Option<&FriendInfo> {
        if let Some(friend) = self
            .friends
            .iter_mut()
            .find(|friend| friend.character_id == friend_id)
        {
            friend.status = status;
            return Some(friend);
        }

        None
    }

    pub fn get_friend(&self, friend_id: CharacterUniqueId) -> Option<&FriendInfo> {
        self.friends
            .iter()
            .find(|friend| friend.character_id == friend_id)
    }

    pub fn append_chat_message(
        &mut self,
        friend_id: CharacterUniqueId,
        from_name: String,
        text: String,
        is_local: bool,
    ) {
        self.chat_histories
            .entry(friend_id)
            .or_default()
            .push(PrivateChatEntry {
                from_name,
                text,
                is_local,
            });
    }

    pub fn request_open_chat(&mut self, friend_id: CharacterUniqueId) {
        if !self.open_chat_requests.contains(&friend_id) {
            self.open_chat_requests.push(friend_id);
        }
    }

    pub fn take_open_chat_requests(&mut self) -> Vec<CharacterUniqueId> {
        std::mem::take(&mut self.open_chat_requests)
    }
}
