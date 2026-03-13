use bevy::{
    ecs::query::WorldQuery,
    prelude::{Assets, Entity, Handle, Or, Query, ResMut, Visibility, With},
};

use rose_data::StatusEffectType;
use rose_game_common::components::{StatusEffects, Team};

use crate::{
    components::{BaseObjectMaterialAlpha, CharacterModel, NpcModel, PlayerCharacter},
    render::ObjectMaterial,
    resources::SelectedTarget,
};

const ALLIED_STEALTH_ALPHA: f32 = 0.3;

#[derive(Copy, Clone, PartialEq, Eq)]
pub enum StealthRenderMode {
    Visible,
    AlliedVisible,
    Hidden,
}

#[derive(WorldQuery)]
pub struct LocalPlayerQuery<'w> {
    entity: Entity,
    team: &'w Team,
}

fn has_stealth_status(status_effects: &StatusEffects) -> bool {
    status_effects.active[StatusEffectType::Disguise].is_some()
        || status_effects.active[StatusEffectType::Transparent].is_some()
}

pub fn get_stealth_render_mode(
    status_effects: Option<&StatusEffects>,
    target_team: Option<&Team>,
    local_player_team: &Team,
    is_local_player: bool,
) -> StealthRenderMode {
    if !status_effects.map_or(false, has_stealth_status) {
        return StealthRenderMode::Visible;
    }

    if is_local_player || target_team.map_or(false, |team| team.id == local_player_team.id) {
        StealthRenderMode::AlliedVisible
    } else {
        StealthRenderMode::Hidden
    }
}

pub fn is_hidden_from_local_player(
    status_effects: Option<&StatusEffects>,
    target_team: Option<&Team>,
    local_player_team: &Team,
    is_local_player: bool,
) -> bool {
    matches!(
        get_stealth_render_mode(
            status_effects,
            target_team,
            local_player_team,
            is_local_player
        ),
        StealthRenderMode::Hidden
    )
}

fn set_part_alpha(
    model_parts: impl Iterator<Item = Entity>,
    query_material: &Query<(&Handle<ObjectMaterial>, &BaseObjectMaterialAlpha)>,
    object_materials: &mut Assets<ObjectMaterial>,
    alpha_multiplier: Option<f32>,
) {
    for part_entity in model_parts {
        let Ok((material_handle, base_alpha)) = query_material.get(part_entity) else {
            continue;
        };

        let Some(material) = object_materials.get_mut(material_handle) else {
            continue;
        };

        let next_alpha = alpha_multiplier.map(|multiplier| base_alpha.value * multiplier);
        if material.alpha_value == next_alpha {
            continue;
        }

        material.alpha_value = next_alpha;
    }
}

pub fn stealth_visibility_system(
    query_player: Query<LocalPlayerQuery, With<PlayerCharacter>>,
    mut query_stealth_targets: Query<
        (
            Entity,
            Option<&CharacterModel>,
            Option<&NpcModel>,
            &StatusEffects,
            &Team,
            &mut Visibility,
            Option<&PlayerCharacter>,
        ),
        Or<(With<CharacterModel>, With<NpcModel>)>,
    >,
    query_material: Query<(&Handle<ObjectMaterial>, &BaseObjectMaterialAlpha)>,
    query_stealth_target: Query<(Option<&StatusEffects>, Option<&Team>)>,
    mut object_materials: ResMut<Assets<ObjectMaterial>>,
    mut selected_target: ResMut<SelectedTarget>,
) {
    let Ok(player) = query_player.get_single() else {
        return;
    };

    let apply_render_mode =
        |entity: Entity,
         visibility: &mut Visibility,
         status_effects: &StatusEffects,
         team: &Team,
         model_parts: Vec<Entity>,
         object_materials: &mut Assets<ObjectMaterial>| {
            let render_mode = get_stealth_render_mode(
                Some(status_effects),
                Some(team),
                player.team,
                entity == player.entity,
            );

            match render_mode {
                StealthRenderMode::Visible => {
                    *visibility = Visibility::Inherited;
                    set_part_alpha(
                        model_parts.into_iter(),
                        &query_material,
                        object_materials,
                        None,
                    );
                }
                StealthRenderMode::AlliedVisible => {
                    *visibility = Visibility::Inherited;
                    set_part_alpha(
                        model_parts.into_iter(),
                        &query_material,
                        object_materials,
                        Some(ALLIED_STEALTH_ALPHA),
                    );
                }
                StealthRenderMode::Hidden => {
                    *visibility = Visibility::Hidden;
                    set_part_alpha(
                        model_parts.into_iter(),
                        &query_material,
                        object_materials,
                        None,
                    );
                }
            }
        };

    for (
        entity,
        character_model,
        npc_model,
        status_effects,
        team,
        mut visibility,
        _player_character,
    ) in query_stealth_targets.iter_mut()
    {
        let Some(model_parts) = character_model
            .map(|character_model| {
                character_model
                    .model_parts
                    .iter()
                    .flat_map(|(_, part)| part.1.iter().copied())
                    .collect()
            })
            .or_else(|| npc_model.map(|npc_model| npc_model.model_parts.iter().copied().collect()))
        else {
            continue;
        };

        apply_render_mode(
            entity,
            &mut visibility,
            status_effects,
            team,
            model_parts,
            &mut object_materials,
        );
    }

    let should_clear_target = |entity: Entity| {
        query_stealth_target
            .get(entity)
            .ok()
            .map_or(false, |(status_effects, team)| {
                is_hidden_from_local_player(
                    status_effects,
                    team,
                    player.team,
                    entity == player.entity,
                )
            })
    };

    if selected_target.hover.map_or(false, should_clear_target) {
        selected_target.hover = None;
    }

    if selected_target.selected.map_or(false, should_clear_target) {
        selected_target.selected = None;
    }
}
