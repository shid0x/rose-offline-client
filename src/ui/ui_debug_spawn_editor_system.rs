use bevy::{
    ecs::system::SystemParam,
    hierarchy::DespawnRecursiveExt,
    input::Input,
    prelude::{
        Assets, Camera, Camera3d, Commands, Component, ComputedVisibility, Entity, GlobalTransform,
        MouseButton, Query, Res, ResMut, State, Transform, Vec3, Visibility, With,
    },
    window::{PrimaryWindow, Window},
};
use bevy_egui::{egui, EguiContexts};
use bevy_rapier3d::prelude::{CollisionGroups, Group, QueryFilter, RapierContext};

use rose_data::NpcId;
use rose_file_readers::{
    types::{Quat4 as IfoQuat4, Vec2 as IfoVec2, Vec3 as IfoVec3},
    IfoFile, IfoMonsterSpawn, IfoMonsterSpawnPoint, IfoObject, RoseFile, RoseFileReader,
    RoseFileWriter,
};
use rose_game_common::components::Npc;
use rose_game_common::messages::client::ClientMessage;

use crate::{
    components::{
        ClientEntityName, ColliderParent, ZoneObject, COLLISION_FILTER_CLICKABLE,
        COLLISION_GROUP_ZONE_TERRAIN,
    },
    resources::{
        AppState, CurrentZone, GameConnection, GameData, PendingNewSpawn, SpawnEditorState,
        VfsResource,
    },
    ui::UiStateDebugWindows,
    zone_loader::{ZoneLoaderAsset, ZoneMonsterSpawn, ZoneMonsterSpawnEntry},
};

#[derive(Component)]
pub struct SpawnEditorPreview {
    pub spawn_index: usize,
}

#[derive(Default)]
pub struct UiDebugSpawnEditorStatus {
    message: String,
}

#[derive(SystemParam)]
pub struct SpawnEditorPickParams<'w, 's> {
    mouse_button_input: Res<'w, Input<MouseButton>>,
    query_window: Query<'w, 's, &'static Window, With<PrimaryWindow>>,
    query_camera: Query<'w, 's, (&'static Camera, &'static GlobalTransform), With<Camera3d>>,
    rapier_context: Res<'w, RapierContext>,
    query_collider_parent: Query<'w, 's, &'static ColliderParent>,
    query_zone_object: Query<'w, 's, &'static ZoneObject>,
    query_preview: Query<'w, 's, (Entity, &'static SpawnEditorPreview, &'static mut Transform)>,
}

#[allow(clippy::too_many_arguments)]
pub fn ui_debug_spawn_editor_system(
    mut commands: Commands,
    mut egui_context: EguiContexts,
    mut ui_state_debug_windows: ResMut<UiStateDebugWindows>,
    mut spawn_editor_state: ResMut<SpawnEditorState>,
    current_zone: Option<Res<CurrentZone>>,
    mut zone_loader_assets: ResMut<Assets<ZoneLoaderAsset>>,
    vfs: Option<Res<VfsResource>>,
    game_data: Res<GameData>,
    app_state: Res<State<AppState>>,
    game_connection: Option<Res<GameConnection>>,
    mut pick_params: SpawnEditorPickParams,
    mut status: bevy::prelude::Local<UiDebugSpawnEditorStatus>,
) {
    if !ui_state_debug_windows.debug_ui_open
        || !spawn_editor_enabled_for_state(&spawn_editor_state, app_state.get())
    {
        despawn_previews(
            &mut commands,
            &mut spawn_editor_state,
            &pick_params.query_preview,
        );
        return;
    }

    let Some(current_zone) = current_zone.as_ref() else {
        despawn_previews(
            &mut commands,
            &mut spawn_editor_state,
            &pick_params.query_preview,
        );
        return;
    };

    let Some(zone_asset) = zone_loader_assets.get_mut(&current_zone.handle) else {
        return;
    };

    sync_preview_entities(
        &mut commands,
        &mut spawn_editor_state,
        &mut pick_params.query_preview,
        &game_data,
        zone_asset,
    );

    pick_spawn_preview(
        &mut ui_state_debug_windows,
        &mut spawn_editor_state,
        &mut egui_context,
        &pick_params.mouse_button_input,
        &pick_params.query_window,
        &pick_params.query_camera,
        &pick_params.rapier_context,
        &pick_params.query_collider_parent,
        &pick_params.query_preview,
    );

    request_new_spawn_at_cursor(
        &mut spawn_editor_state,
        &mut egui_context,
        &pick_params.mouse_button_input,
        &pick_params.query_window,
        &pick_params.query_camera,
        &pick_params.rapier_context,
        &pick_params.query_collider_parent,
        &pick_params.query_zone_object,
        zone_asset,
    );

    draw_new_spawn_prompt(
        &mut egui_context,
        &mut spawn_editor_state,
        &mut ui_state_debug_windows,
        zone_asset,
        &vfs,
        &game_data,
        game_connection.as_deref(),
        current_zone.id,
        &mut status,
    );

    if !ui_state_debug_windows.spawn_editor_open {
        return;
    }

    egui::Window::new("Spawn Editor")
        .vscroll(true)
        .resizable(true)
        .default_width(620.0)
        .default_height(520.0)
        .open(&mut ui_state_debug_windows.spawn_editor_open)
        .show(egui_context.ctx_mut(), |ui| {
            if zone_asset.monster_spawns.is_empty() {
                ui.label("This zone has no monster spawn points.");
                return;
            }

            let Some(selected_index) = spawn_editor_state
                .selected_spawn
                .filter(|index| *index < zone_asset.monster_spawns.len())
            else {
                ui.label("Click a spawn preview in the world to edit it.");
                return;
            };

            let object_offset = zone_object_offset(zone_asset);
            let mut save_spawn = None;
            let mut reload_spawn = None;
            {
                let spawn = &mut zone_asset.monster_spawns[selected_index];
                draw_spawn_editor(ui, selected_index, spawn, &game_data);

                ui.horizontal(|ui| {
                    if ui.button("Save IFO").clicked() {
                        save_spawn = Some(spawn.clone());
                    }
                    if ui.button("Reload Server").clicked() {
                        reload_spawn = Some(spawn.clone());
                    }
                    if !status.message.is_empty() {
                        ui.separator();
                        ui.label(&status.message);
                    }
                });
            }

            if let Some(spawn) = save_spawn {
                if let Some(vfs) = vfs.as_ref() {
                    match save_spawn_to_ifo(&vfs.vfs, &spawn, object_offset) {
                        Ok(()) => {
                            status.message = format!(
                                "Saved {}; {}",
                                spawn.source_ifo_path.to_string_lossy(),
                                send_spawn_reload_command(
                                    game_connection.as_deref(),
                                    current_zone.id.get(),
                                    spawn.source_block_x,
                                    spawn.source_block_y,
                                )
                            );
                        }
                        Err(error) => {
                            status.message = format!("Save failed: {error:#}");
                        }
                    }
                } else {
                    status.message = "Save failed: no VFS resource".to_string();
                }
            }

            if let Some(spawn) = reload_spawn {
                status.message = send_spawn_reload_command(
                    game_connection.as_deref(),
                    current_zone.id.get(),
                    spawn.source_block_x,
                    spawn.source_block_y,
                );
            }
        });
}

fn spawn_editor_enabled_for_state(
    spawn_editor_state: &SpawnEditorState,
    app_state: &AppState,
) -> bool {
    matches!(app_state, AppState::ZoneViewer)
        || (matches!(app_state, AppState::Game) && spawn_editor_state.active)
}

#[allow(clippy::too_many_arguments)]
fn request_new_spawn_at_cursor(
    spawn_editor_state: &mut SpawnEditorState,
    egui_context: &mut EguiContexts,
    mouse_button_input: &Input<MouseButton>,
    query_window: &Query<&Window, With<PrimaryWindow>>,
    query_camera: &Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    rapier_context: &RapierContext,
    query_collider_parent: &Query<&ColliderParent>,
    query_zone_object: &Query<&ZoneObject>,
    zone_asset: &ZoneLoaderAsset,
) {
    if !mouse_button_input.just_pressed(MouseButton::Right)
        || egui_context.ctx_mut().wants_pointer_input()
    {
        return;
    }

    let Ok(window) = query_window.get_single() else {
        return;
    };
    let Some(cursor_position) = window.cursor_position() else {
        return;
    };
    let Ok((camera, camera_transform)) = query_camera.get_single() else {
        return;
    };
    let Some(ray) = camera.viewport_to_world(camera_transform, cursor_position) else {
        return;
    };

    let Some((collider_entity, distance)) = rapier_context.cast_ray(
        ray.origin,
        ray.direction,
        10000000.0,
        false,
        QueryFilter::new().groups(CollisionGroups::new(
            COLLISION_FILTER_CLICKABLE,
            COLLISION_GROUP_ZONE_TERRAIN,
        )),
    ) else {
        return;
    };

    let hit_entity = query_collider_parent
        .get(collider_entity)
        .map_or(collider_entity, |collider_parent| collider_parent.entity);
    let Ok(ZoneObject::Terrain(terrain)) = query_zone_object.get(hit_entity) else {
        return;
    };

    let block_index = terrain.block_x as usize + terrain.block_y as usize * 64;
    let Some(block) = zone_asset
        .blocks
        .get(block_index)
        .and_then(|block| block.as_ref())
    else {
        return;
    };

    let hit_position = ray.get_point(distance);
    spawn_editor_state.pending_new_spawn = Some(PendingNewSpawn {
        source_ifo_path: block.ifo_path.clone(),
        source_block_x: terrain.block_x as usize,
        source_block_y: terrain.block_y as usize,
        position: Vec3::new(
            hit_position.x * 100.0,
            -hit_position.z * 100.0,
            hit_position.y * 100.0,
        ),
        name: String::new(),
        npc_id: 1,
        count: 1,
        range: 30,
        interval: 10,
        limit_count: 5,
        tactic_points: 100,
    });
}

#[allow(clippy::too_many_arguments)]
fn draw_new_spawn_prompt(
    egui_context: &mut EguiContexts,
    spawn_editor_state: &mut SpawnEditorState,
    ui_state_debug_windows: &mut UiStateDebugWindows,
    zone_asset: &mut ZoneLoaderAsset,
    vfs: &Option<Res<VfsResource>>,
    game_data: &GameData,
    game_connection: Option<&GameConnection>,
    zone_id: rose_data::ZoneId,
    status: &mut UiDebugSpawnEditorStatus,
) {
    let Some(mut pending) = spawn_editor_state.pending_new_spawn.take() else {
        return;
    };

    let mut keep_open = true;
    let mut add_clicked = false;
    let mut cancel_clicked = false;
    egui::Window::new("New Spawn")
        .collapsible(false)
        .resizable(false)
        .open(&mut keep_open)
        .show(egui_context.ctx_mut(), |ui| {
            ui.label(format!(
                "Block {}_{}",
                pending.source_block_x, pending.source_block_y
            ));
            ui.label(pending.source_ifo_path.to_string_lossy());
            egui::Grid::new("new_spawn_fields")
                .num_columns(2)
                .spacing([12.0, 6.0])
                .show(ui, |ui| {
                    ui.label("Name");
                    ui.text_edit_singleline(&mut pending.name);
                    ui.end_row();

                    ui.label("Monster");
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::DragValue::new(&mut pending.npc_id)
                                .speed(1.0)
                                .clamp_range(1..=u32::MAX),
                        );
                        ui.label(npc_display_name(pending.npc_id, game_data));
                    });
                    ui.end_row();

                    ui.label("Batch Count");
                    ui.add(
                        egui::DragValue::new(&mut pending.count)
                            .speed(1.0)
                            .clamp_range(1..=1000),
                    );
                    ui.end_row();

                    ui.label("Position");
                    ui.horizontal(|ui| {
                        ui.add(egui::DragValue::new(&mut pending.position.x).prefix("x "));
                        ui.add(egui::DragValue::new(&mut pending.position.y).prefix("y "));
                        ui.add(egui::DragValue::new(&mut pending.position.z).prefix("z "));
                    });
                    ui.end_row();

                    ui.label("Range");
                    ui.add(egui::DragValue::new(&mut pending.range).clamp_range(0..=10000));
                    ui.end_row();

                    ui.label("Spawn Check Seconds");
                    ui.add(egui::DragValue::new(&mut pending.interval).clamp_range(0..=3600));
                    ui.end_row();

                    ui.label("Max Alive");
                    ui.add(egui::DragValue::new(&mut pending.limit_count).clamp_range(1..=1000));
                    ui.end_row();

                    ui.label("Tactic Points");
                    ui.add(egui::DragValue::new(&mut pending.tactic_points).clamp_range(1..=10000));
                    ui.end_row();
                });

            ui.horizontal(|ui| {
                add_clicked = ui.button("Add Spawn").clicked();
                cancel_clicked = ui.button("Cancel").clicked();
            });
        });

    if cancel_clicked || !keep_open {
        return;
    }

    if add_clicked {
        if let Some(vfs) = vfs.as_ref() {
            match add_spawn_to_ifo(&vfs.vfs, zone_asset, pending) {
                Ok(spawn_index) => {
                    let spawn = &zone_asset.monster_spawns[spawn_index];
                    spawn_editor_state.selected_spawn = Some(spawn_index);
                    ui_state_debug_windows.spawn_editor_open = true;
                    status.message = format!(
                        "Added spawn; {}",
                        send_spawn_reload_command(
                            game_connection,
                            zone_id.get(),
                            spawn.source_block_x,
                            spawn.source_block_y,
                        )
                    );
                }
                Err(error) => {
                    status.message = format!("Add failed: {error:#}");
                }
            }
        } else {
            status.message = "Add failed: no VFS resource".to_string();
        }
    } else {
        spawn_editor_state.pending_new_spawn = Some(pending);
    }
}

fn send_spawn_reload_command(
    game_connection: Option<&GameConnection>,
    zone_id: u16,
    block_x: usize,
    block_y: usize,
) -> String {
    if let Some(game_connection) = game_connection {
        match game_connection.client_message_tx.send(ClientMessage::Chat {
            text: format!("/spawn_reload {zone_id} {block_x} {block_y}"),
        }) {
            Ok(()) => {
                format!("requested server reload for zone {zone_id} block {block_x}_{block_y}")
            }
            Err(_) => "server reload request failed: game connection is closed".to_string(),
        }
    } else {
        "no server reload because game server is not connected".to_string()
    }
}

fn sync_preview_entities(
    commands: &mut Commands,
    spawn_editor_state: &mut SpawnEditorState,
    query_preview: &mut Query<(Entity, &SpawnEditorPreview, &mut Transform)>,
    game_data: &GameData,
    zone_asset: &ZoneLoaderAsset,
) {
    spawn_editor_state
        .preview_entities
        .resize(zone_asset.monster_spawns.len(), None);
    spawn_editor_state
        .preview_npc_ids
        .resize(zone_asset.monster_spawns.len(), None);

    for spawn_index in 0..zone_asset.monster_spawns.len() {
        let spawn = &zone_asset.monster_spawns[spawn_index];
        let Some(preview_npc_id) = preview_npc_id(spawn) else {
            despawn_preview(commands, spawn_editor_state, query_preview, spawn_index);
            continue;
        };
        let Some(npc_id) = NpcId::new(preview_npc_id as u16) else {
            despawn_preview(commands, spawn_editor_state, query_preview, spawn_index);
            continue;
        };

        let transform = Transform::from_translation(Vec3::new(
            spawn.position.x / 100.0,
            spawn.position.z / 100.0,
            -spawn.position.y / 100.0,
        ));

        if spawn_editor_state.preview_npc_ids[spawn_index] == Some(preview_npc_id) {
            if let Some(preview_entity) = spawn_editor_state.preview_entities[spawn_index] {
                if let Ok((_, _, mut preview_transform)) = query_preview.get_mut(preview_entity) {
                    *preview_transform = transform;
                    continue;
                }
            }
        }

        despawn_preview(commands, spawn_editor_state, query_preview, spawn_index);
        let name = game_data
            .npcs
            .get_npc(npc_id)
            .map(|npc| npc.name.to_string())
            .unwrap_or_else(|| format!("??? [{}]", preview_npc_id));
        let entity = commands
            .spawn((
                SpawnEditorPreview { spawn_index },
                ClientEntityName { name },
                Npc::new(npc_id, 0),
                Visibility::default(),
                ComputedVisibility::default(),
                GlobalTransform::default(),
                transform,
            ))
            .id();

        spawn_editor_state.preview_entities[spawn_index] = Some(entity);
        spawn_editor_state.preview_npc_ids[spawn_index] = Some(preview_npc_id);
    }

    for spawn_index in zone_asset.monster_spawns.len()..spawn_editor_state.preview_entities.len() {
        despawn_preview(commands, spawn_editor_state, query_preview, spawn_index);
    }
    spawn_editor_state
        .preview_entities
        .truncate(zone_asset.monster_spawns.len());
    spawn_editor_state
        .preview_npc_ids
        .truncate(zone_asset.monster_spawns.len());
}

#[allow(clippy::too_many_arguments)]
fn pick_spawn_preview(
    ui_state_debug_windows: &mut UiStateDebugWindows,
    spawn_editor_state: &mut SpawnEditorState,
    egui_context: &mut EguiContexts,
    mouse_button_input: &Input<MouseButton>,
    query_window: &Query<&Window, With<PrimaryWindow>>,
    query_camera: &Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    rapier_context: &RapierContext,
    query_collider_parent: &Query<&ColliderParent>,
    query_preview: &Query<(Entity, &SpawnEditorPreview, &mut Transform)>,
) {
    if !mouse_button_input.just_pressed(MouseButton::Left)
        || egui_context.ctx_mut().wants_pointer_input()
    {
        return;
    }

    let Ok(window) = query_window.get_single() else {
        return;
    };
    let Some(cursor_position) = window.cursor_position() else {
        return;
    };
    let Ok((camera, camera_transform)) = query_camera.get_single() else {
        return;
    };
    let Some(ray) = camera.viewport_to_world(camera_transform, cursor_position) else {
        return;
    };

    if let Some((collider_entity, _distance)) = rapier_context.cast_ray(
        ray.origin,
        ray.direction,
        10000000.0,
        false,
        QueryFilter::new().groups(CollisionGroups::new(
            COLLISION_FILTER_CLICKABLE,
            Group::all(),
        )),
    ) {
        let hit_entity = query_collider_parent
            .get(collider_entity)
            .map_or(collider_entity, |collider_parent| collider_parent.entity);

        if let Ok((_, preview, _)) = query_preview.get(hit_entity) {
            spawn_editor_state.selected_spawn = Some(preview.spawn_index);
            ui_state_debug_windows.spawn_editor_open = true;
        }
    }
}

fn despawn_previews(
    commands: &mut Commands,
    spawn_editor_state: &mut SpawnEditorState,
    query_preview: &Query<(Entity, &SpawnEditorPreview, &mut Transform)>,
) {
    for preview_entity in spawn_editor_state
        .preview_entities
        .drain(..)
        .flatten()
        .collect::<Vec<_>>()
    {
        if query_preview.get(preview_entity).is_ok() {
            commands.entity(preview_entity).despawn_recursive();
        }
    }

    spawn_editor_state.preview_npc_ids.clear();
}

fn despawn_preview(
    commands: &mut Commands,
    spawn_editor_state: &mut SpawnEditorState,
    query_preview: &Query<(Entity, &SpawnEditorPreview, &mut Transform)>,
    spawn_index: usize,
) {
    if let Some(preview_entity) = spawn_editor_state
        .preview_entities
        .get_mut(spawn_index)
        .and_then(Option::take)
    {
        if query_preview.get(preview_entity).is_ok() {
            commands.entity(preview_entity).despawn_recursive();
        }
    }

    if let Some(preview_npc_id) = spawn_editor_state.preview_npc_ids.get_mut(spawn_index) {
        *preview_npc_id = None;
    }
}

fn draw_spawn_editor(
    ui: &mut egui::Ui,
    selected_index: usize,
    spawn: &mut ZoneMonsterSpawn,
    game_data: &GameData,
) {
    ui.heading(format!(
        "Spawn {} - {}",
        selected_index,
        display_spawn_name(spawn, game_data)
    ));
    egui::Grid::new("spawn_editor_fields")
        .num_columns(2)
        .spacing([12.0, 6.0])
        .show(ui, |ui| {
            ui.label("Name");
            ui.add(egui::TextEdit::singleline(&mut spawn.name).hint_text("optional IFO name"));
            ui.end_row();

            ui.label("Preview");
            ui.label(display_preview_monster(spawn, game_data));
            ui.end_row();

            ui.label("IFO");
            ui.label(spawn.source_ifo_path.to_string_lossy());
            ui.end_row();

            ui.label("Position");
            ui.horizontal(|ui| {
                ui.add(
                    egui::DragValue::new(&mut spawn.position.x)
                        .speed(10.0)
                        .prefix("x "),
                );
                ui.add(
                    egui::DragValue::new(&mut spawn.position.y)
                        .speed(10.0)
                        .prefix("y "),
                );
                ui.add(
                    egui::DragValue::new(&mut spawn.position.z)
                        .speed(10.0)
                        .prefix("z "),
                );
            });
            ui.end_row();

            ui.label("Range");
            ui.add(
                egui::DragValue::new(&mut spawn.range)
                    .speed(1.0)
                    .clamp_range(0..=10000),
            );
            ui.end_row();

            ui.label("Spawn Check Seconds");
            ui.add(
                egui::DragValue::new(&mut spawn.interval)
                    .speed(1.0)
                    .clamp_range(0..=3600),
            );
            ui.end_row();

            ui.label("Max Alive");
            ui.add(
                egui::DragValue::new(&mut spawn.limit_count)
                    .speed(1.0)
                    .clamp_range(0..=1000),
            );
            ui.end_row();

            ui.label("Tactic Points");
            ui.add(
                egui::DragValue::new(&mut spawn.tactic_points)
                    .speed(1.0)
                    .clamp_range(1..=10000),
            );
            ui.end_row();
        });

    ui.separator();
    draw_spawn_entries(ui, "Basic Spawns", &mut spawn.basic_spawns, game_data);
    ui.separator();
    draw_spawn_entries(ui, "Tactic Spawns", &mut spawn.tactic_spawns, game_data);
}

fn draw_spawn_entries(
    ui: &mut egui::Ui,
    heading: &str,
    entries: &mut Vec<ZoneMonsterSpawnEntry>,
    game_data: &GameData,
) {
    ui.heading(heading);
    let mut remove_index = None;
    egui::Grid::new(heading)
        .num_columns(4)
        .spacing([8.0, 4.0])
        .show(ui, |ui| {
            ui.label("Monster");
            ui.label("NPC ID");
            ui.label("Batch Count");
            ui.label("");
            ui.end_row();

            for (index, entry) in entries.iter_mut().enumerate() {
                ui.label(npc_display_name(entry.npc_id, game_data));
                ui.add(
                    egui::DragValue::new(&mut entry.npc_id)
                        .speed(1.0)
                        .clamp_range(0..=u32::MAX),
                );
                ui.add(
                    egui::DragValue::new(&mut entry.count)
                        .speed(1.0)
                        .clamp_range(0..=1000),
                );
                if ui.button("Remove").clicked() {
                    remove_index = Some(index);
                }
                ui.end_row();
            }
        });

    if let Some(index) = remove_index {
        entries.remove(index);
    }

    if ui.button(format!("Add {heading} Entry")).clicked() {
        entries.push(ZoneMonsterSpawnEntry {
            name: String::new(),
            npc_id: 1,
            count: 1,
        });
    }
}

fn display_spawn_name(spawn: &ZoneMonsterSpawn, game_data: &GameData) -> String {
    if !spawn_name_is_placeholder(&spawn.name) {
        return spawn.name.clone();
    }

    display_preview_monster(spawn, game_data)
}

fn spawn_name_is_placeholder(name: &str) -> bool {
    let name = name.trim();
    name.is_empty() || name.eq_ignore_ascii_case("untitled")
}

fn display_preview_monster(spawn: &ZoneMonsterSpawn, game_data: &GameData) -> String {
    let Some(npc_id) = preview_npc_id(spawn) else {
        return "No monsters".to_string();
    };

    let name = NpcId::new(npc_id as u16)
        .and_then(|npc_id| game_data.npcs.get_npc(npc_id))
        .map(|npc| npc.name)
        .filter(|name| !name.is_empty())
        .unwrap_or("Unknown");

    format!("[{}] {}", npc_id, name)
}

fn npc_display_name(npc_id: u32, game_data: &GameData) -> String {
    let name = NpcId::new(npc_id as u16)
        .and_then(|npc_id| game_data.npcs.get_npc(npc_id))
        .map(|npc| npc.name)
        .filter(|name| !name.is_empty())
        .unwrap_or("Unknown");

    format!("[{}] {}", npc_id, name)
}

fn preview_npc_id(spawn: &ZoneMonsterSpawn) -> Option<u32> {
    spawn
        .basic_spawns
        .first()
        .or_else(|| spawn.tactic_spawns.first())
        .map(|entry| entry.npc_id)
}

fn zone_object_offset(zone_asset: &ZoneLoaderAsset) -> Vec3 {
    Vec3::new(
        (64.0 / 2.0) * (zone_asset.zon.grid_size * zone_asset.zon.grid_per_patch * 16.0)
            + (zone_asset.zon.grid_size * zone_asset.zon.grid_per_patch * 16.0) / 2.0,
        (64.0 / 2.0) * (zone_asset.zon.grid_size * zone_asset.zon.grid_per_patch * 16.0)
            + (zone_asset.zon.grid_size * zone_asset.zon.grid_per_patch * 16.0) / 2.0,
        0.0,
    )
}

fn add_spawn_to_ifo(
    vfs: &rose_file_readers::VirtualFilesystem,
    zone_asset: &mut ZoneLoaderAsset,
    pending: PendingNewSpawn,
) -> Result<usize, anyhow::Error> {
    let file = vfs.open_file(pending.source_ifo_path.as_path())?;
    let mut ifo = IfoFile::read(RoseFileReader::from(&file), &Default::default())?;
    let object_offset = zone_object_offset(zone_asset);
    let source_spawn_index = ifo.monster_spawns.len();

    ifo.monster_spawns.push(IfoMonsterSpawnPoint {
        object: IfoObject {
            object_name: pending.name.clone(),
            minimap_position: IfoVec2 { x: 0, y: 0 },
            object_type: 0,
            object_id: 0,
            warp_id: 0,
            event_id: 0,
            position: IfoVec3 {
                x: pending.position.x - object_offset.x,
                y: pending.position.y - object_offset.y,
                z: pending.position.z - object_offset.z,
            },
            rotation: IfoQuat4 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
                w: 1.0,
            },
            scale: IfoVec3 {
                x: 1.0,
                y: 1.0,
                z: 1.0,
            },
        },
        name: pending.name.clone(),
        basic_spawns: vec![IfoMonsterSpawn {
            name: String::new(),
            id: pending.npc_id,
            count: pending.count,
        }],
        tactic_spawns: Vec::new(),
        interval: pending.interval,
        limit_count: pending.limit_count,
        range: pending.range,
        tactic_points: pending.tactic_points,
    });

    let mut writer = RoseFileWriter::default();
    ifo.write(&mut writer, &())?;
    vfs.write_existing_file(pending.source_ifo_path.as_path(), writer.buffer.as_ref())?;

    let verify_file = vfs.open_file(pending.source_ifo_path.as_path())?;
    let verify_ifo = IfoFile::read(RoseFileReader::from(&verify_file), &Default::default())?;
    let Some(saved_spawn) = verify_ifo.monster_spawns.get(source_spawn_index) else {
        anyhow::bail!(
            "spawn index {} was not found after saving {}",
            source_spawn_index,
            pending.source_ifo_path.to_string_lossy()
        );
    };
    if saved_spawn
        .basic_spawns
        .first()
        .is_none_or(|entry| entry.id != pending.npc_id || entry.count != pending.count)
    {
        anyhow::bail!(
            "spawn index {} did not round-trip after saving {}",
            source_spawn_index,
            pending.source_ifo_path.to_string_lossy()
        );
    }

    let new_spawn_index = zone_asset.monster_spawns.len();
    zone_asset.monster_spawns.push(ZoneMonsterSpawn {
        source_ifo_path: pending.source_ifo_path,
        source_block_x: pending.source_block_x,
        source_block_y: pending.source_block_y,
        source_spawn_index,
        position: pending.position,
        name: pending.name,
        range: pending.range,
        interval: pending.interval,
        limit_count: pending.limit_count,
        tactic_points: pending.tactic_points,
        basic_spawns: vec![ZoneMonsterSpawnEntry {
            name: String::new(),
            npc_id: pending.npc_id,
            count: pending.count,
        }],
        tactic_spawns: Vec::new(),
    });

    Ok(new_spawn_index)
}

fn save_spawn_to_ifo(
    vfs: &rose_file_readers::VirtualFilesystem,
    spawn: &ZoneMonsterSpawn,
    object_offset: Vec3,
) -> Result<(), anyhow::Error> {
    let file = vfs.open_file(spawn.source_ifo_path.as_path())?;
    let mut ifo = IfoFile::read(RoseFileReader::from(&file), &Default::default())?;
    let Some(ifo_spawn) = ifo.monster_spawns.get_mut(spawn.source_spawn_index) else {
        anyhow::bail!(
            "spawn index {} no longer exists in {}",
            spawn.source_spawn_index,
            spawn.source_ifo_path.to_string_lossy()
        );
    };

    ifo_spawn.name = spawn.name.clone();
    ifo_spawn.object.position.x = spawn.position.x - object_offset.x;
    ifo_spawn.object.position.y = spawn.position.y - object_offset.y;
    ifo_spawn.object.position.z = spawn.position.z - object_offset.z;
    ifo_spawn.range = spawn.range;
    ifo_spawn.interval = spawn.interval;
    ifo_spawn.limit_count = spawn.limit_count;
    ifo_spawn.tactic_points = spawn.tactic_points;
    ifo_spawn.basic_spawns = spawn
        .basic_spawns
        .iter()
        .map(|entry| IfoMonsterSpawn {
            name: entry.name.clone(),
            id: entry.npc_id,
            count: entry.count,
        })
        .collect();
    ifo_spawn.tactic_spawns = spawn
        .tactic_spawns
        .iter()
        .map(|entry| IfoMonsterSpawn {
            name: entry.name.clone(),
            id: entry.npc_id,
            count: entry.count,
        })
        .collect();

    let mut writer = RoseFileWriter::default();
    ifo.write(&mut writer, &())?;
    vfs.write_existing_file(spawn.source_ifo_path.as_path(), writer.buffer.as_ref())
}
