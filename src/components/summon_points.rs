use bevy::ecs::prelude::Component;

#[derive(Component, Clone, Copy, Debug, Default)]
pub struct SummonPoints {
    pub used_points: u16,
    pub max_points: u16,
}
