use bevy::prelude::Resource;

use crate::AntiAliasConfig;

#[derive(Resource)]
pub struct RenderConfiguration {
    pub passthrough_terrain_textures: bool,
    pub trail_effect_duration_multiplier: f32,
    pub anti_aliasing: AntiAliasConfig,
}
