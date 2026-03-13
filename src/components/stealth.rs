use bevy::prelude::Component;

#[derive(Component)]
pub struct BaseObjectMaterialAlpha {
    pub value: f32,
}

impl BaseObjectMaterialAlpha {
    pub fn new(value: f32) -> Self {
        Self { value }
    }
}
