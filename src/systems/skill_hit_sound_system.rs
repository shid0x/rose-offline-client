use bevy::prelude::{AssetServer, Commands, EventReader, GlobalTransform, Query, Res, Transform};

use crate::{
    audio::SpatialSound,
    components::{PlayerCharacter, SoundCategory},
    events::SkillHitSoundEvent,
    resources::{GameData, SoundCache, SoundSettings},
};

pub fn skill_hit_sound_system(
    mut commands: Commands,
    mut skill_hit_sound_events: EventReader<SkillHitSoundEvent>,
    query_entities: Query<(Option<&PlayerCharacter>, &GlobalTransform)>,
    game_data: Res<GameData>,
    sound_settings: Res<SoundSettings>,
    sound_cache: Res<SoundCache>,
    asset_server: Res<AssetServer>,
) {
    for event in skill_hit_sound_events.iter() {
        let Some(skill_data) = game_data.skills.get_skill(event.skill_id) else {
            log::warn!(
                "Unable to play AOE hit sound for unknown skill id {}",
                event.skill_id.get()
            );
            continue;
        };

        let Some(hit_sound_id) = skill_data.hit_sound_id else {
            log::debug!(
                "AOE skill {} has no hit_sound_id configured",
                event.skill_id.get()
            );
            continue;
        };

        let Some(sound_data) = game_data.sounds.get_sound(hit_sound_id) else {
            log::warn!(
                "Unable to resolve hit sound {} for AOE skill {}",
                hit_sound_id.get(),
                event.skill_id.get()
            );
            continue;
        };

        let Ok((defender_player, defender_transform)) = query_entities.get(event.defender) else {
            log::debug!(
                "Skipping AOE hit sound for missing defender entity {:?}",
                event.defender
            );
            continue;
        };

        let attacker_is_player = query_entities
            .get(event.attacker)
            .ok()
            .map_or(false, |(player, _)| player.is_some());

        let sound_category = if attacker_is_player || defender_player.is_some() {
            SoundCategory::PlayerCombat
        } else {
            SoundCategory::OtherCombat
        };

        commands.spawn((
            sound_category,
            sound_settings.gain(sound_category),
            SpatialSound::new(sound_cache.load(sound_data, &asset_server)),
            Transform::from_translation(defender_transform.translation()),
            GlobalTransform::from_translation(defender_transform.translation()),
        ));
    }
}
