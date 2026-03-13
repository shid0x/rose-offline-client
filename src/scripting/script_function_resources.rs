use bevy::{ecs::system::SystemParam, prelude::Res};

use crate::resources::{CurrentZone, GameConnection, GameData, WorldTime};

#[derive(SystemParam)]
pub struct ScriptFunctionResources<'w, 's> {
    pub game_connection: Option<Res<'w, GameConnection>>,
    pub game_data: Res<'w, GameData>,
    pub current_zone: Option<Res<'w, CurrentZone>>,
    pub world_time: Res<'w, WorldTime>,

    #[system_param(ignore)]
    pub phantom: std::marker::PhantomData<&'s ()>,
}
