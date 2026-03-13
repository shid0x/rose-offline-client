use bevy::prelude::{Entity, Event};

#[derive(Event)]
pub struct WorldChatBubbleEvent {
    pub entity: Entity,
    pub text: String,
}
