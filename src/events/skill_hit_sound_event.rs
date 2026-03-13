use bevy::prelude::{Entity, Event};

use rose_data::SkillId;

#[derive(Event, Copy, Clone, Debug)]
pub struct SkillHitSoundEvent {
    pub attacker: Entity,
    pub defender: Entity,
    pub skill_id: SkillId,
}

impl SkillHitSoundEvent {
    pub fn new(attacker: Entity, defender: Entity, skill_id: SkillId) -> Self {
        Self {
            attacker,
            defender,
            skill_id,
        }
    }
}
