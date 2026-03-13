use bevy::prelude::Resource;

#[derive(Default, Resource)]
pub struct ConversationDialogState {
    pub is_open: bool,
}
