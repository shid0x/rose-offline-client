use bevy::{
    hierarchy::DespawnRecursiveExt,
    prelude::{
        AssetServer, Assets, Changed, Commands, Entity, Or, Query, RemovedComponents, Res, ResMut,
        Without,
    },
    render::mesh::skinning::{SkinnedMesh, SkinnedMeshInverseBindposes},
};

use rose_game_common::components::{CharacterInfo, Equipment};

use crate::{
    components::{
        CharacterBlinkTimer, CharacterModel, DummyBoneOffset, ModelHeight, PersonalStore,
        RemoveColliderCommand,
    },
    model_loader::ModelLoader,
    render::{EffectMeshMaterial, ObjectMaterial, ParticleMaterial},
};

pub fn character_model_update_system(
    mut commands: Commands,
    mut query: Query<
        (
            Entity,
            &CharacterInfo,
            &Equipment,
            Option<&mut CharacterModel>,
            Option<&mut DummyBoneOffset>,
            Option<&mut SkinnedMesh>,
            Option<&PersonalStore>,
        ),
        Or<(
            Changed<CharacterInfo>,
            Changed<Equipment>,
            Changed<PersonalStore>,
        )>,
    >,
    query_restore_from_personal_store: Query<(&CharacterInfo, &Equipment), Without<PersonalStore>>,
    mut removed_personal_store: RemovedComponents<PersonalStore>,
    query_entities: Query<Entity>,
    asset_server: Res<AssetServer>,
    model_loader: Res<ModelLoader>,
    mut object_materials: ResMut<Assets<ObjectMaterial>>,
    mut particle_materials: ResMut<Assets<ParticleMaterial>>,
    mut effect_mesh_materials: ResMut<Assets<EffectMeshMaterial>>,
    mut skinned_mesh_inverse_bindposes_assets: ResMut<Assets<SkinnedMeshInverseBindposes>>,
) {
    for (
        entity,
        character_info,
        equipment,
        mut current_character_model,
        current_dummy_bone_offset,
        mut current_skinned_mesh,
        personal_store,
    ) in query.iter_mut()
    {
        if personal_store.is_some() {
            if let Some(current_character_model) = current_character_model.as_mut() {
                // Personal store has its own model; remove character model to avoid overlap.
                for (_, (_, model_parts)) in current_character_model.model_parts.iter_mut() {
                    for part_entity in model_parts.drain(..) {
                        if query_entities.get(part_entity).is_ok() {
                            commands.entity(part_entity).despawn_recursive();
                        }
                    }
                }
            }

            if let Some(current_skinned_mesh) = current_skinned_mesh.as_mut() {
                for bone_entity in current_skinned_mesh.joints.drain(..) {
                    if query_entities.get(bone_entity).is_ok() {
                        commands.entity(bone_entity).despawn_recursive();
                    }
                }
            }

            commands
                .entity(entity)
                .remove_and_despawn_collider()
                .remove::<CharacterBlinkTimer>()
                .remove::<CharacterModel>()
                .remove::<SkinnedMesh>()
                .remove::<DummyBoneOffset>();
            continue;
        }

        if let Some(current_character_model) = current_character_model.as_mut() {
            if character_info.gender == current_character_model.gender {
                // Update existing model
                model_loader.update_character_equipment(
                    &mut commands,
                    &asset_server,
                    &mut object_materials,
                    &mut particle_materials,
                    &mut effect_mesh_materials,
                    entity,
                    character_info,
                    equipment,
                    &mut *current_character_model,
                    &current_dummy_bone_offset.unwrap(),
                    &current_skinned_mesh.unwrap(),
                );
                commands
                    .entity(entity)
                    .remove_and_despawn_collider()
                    .remove::<ModelHeight>();
                continue;
            }

            // Despawn model parts
            for (_, (_, model_parts)) in current_character_model.model_parts.iter_mut() {
                for part_entity in model_parts.drain(..) {
                    if query_entities.get(part_entity).is_ok() {
                        commands.entity(part_entity).despawn_recursive();
                    }
                }
            }

            // Despawn model skeleton
            if let Some(current_skinned_mesh) = current_skinned_mesh.as_mut() {
                for bone_entity in current_skinned_mesh.joints.drain(..) {
                    if query_entities.get(bone_entity).is_ok() {
                        commands.entity(bone_entity).despawn_recursive();
                    }
                }
            }

            // Remove the old model collider
            commands.entity(entity).remove_and_despawn_collider();
        }

        let (character_model, skinned_mesh, dummy_bone_offset) = model_loader
            .spawn_character_model(
                &mut commands,
                &asset_server,
                &mut object_materials,
                &mut particle_materials,
                &mut effect_mesh_materials,
                &mut skinned_mesh_inverse_bindposes_assets,
                entity,
                character_info,
                equipment,
            );

        let mut entity_commands = commands.entity(entity);
        entity_commands
            .insert(CharacterBlinkTimer::new())
            .remove_and_despawn_collider();

        if let Some(mut current_character_model) = current_character_model {
            *current_character_model = character_model;
        } else {
            entity_commands.insert(character_model);
        }

        if let Some(mut current_skinned_mesh) = current_skinned_mesh {
            *current_skinned_mesh = skinned_mesh;
        } else {
            entity_commands.insert(skinned_mesh);
        }

        if let Some(mut current_dummy_bone_offset) = current_dummy_bone_offset {
            *current_dummy_bone_offset = dummy_bone_offset;
        } else {
            entity_commands.insert(dummy_bone_offset);
        }
    }

    // RemovedComponents<T> does not trigger Changed<T>, so explicitly rebuild character model
    // when PersonalStore is removed (shop close).
    for entity in removed_personal_store.iter() {
        let Ok((character_info, equipment)) = query_restore_from_personal_store.get(entity) else {
            continue;
        };

        let (character_model, skinned_mesh, dummy_bone_offset) = model_loader
            .spawn_character_model(
                &mut commands,
                &asset_server,
                &mut object_materials,
                &mut particle_materials,
                &mut effect_mesh_materials,
                &mut skinned_mesh_inverse_bindposes_assets,
                entity,
                character_info,
                equipment,
            );

        commands
            .entity(entity)
            .insert(CharacterBlinkTimer::new())
            .remove_and_despawn_collider()
            .remove::<ModelHeight>()
            .insert(character_model)
            .insert(skinned_mesh)
            .insert(dummy_bone_offset);
    }
}
