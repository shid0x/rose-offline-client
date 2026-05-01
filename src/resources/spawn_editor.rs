use bevy::prelude::{Entity, Resource, Vec3};
use std::path::PathBuf;

pub struct PendingNewSpawn {
    pub source_ifo_path: PathBuf,
    pub source_block_x: usize,
    pub source_block_y: usize,
    pub position: Vec3,
    pub name: String,
    pub npc_id: u32,
    pub count: u32,
    pub range: u32,
    pub interval: u32,
    pub limit_count: u32,
    pub tactic_points: u32,
}

#[derive(Default, Resource)]
pub struct SpawnEditorState {
    pub active: bool,
    pub selected_spawn: Option<usize>,
    pub preview_entities: Vec<Option<Entity>>,
    pub preview_npc_ids: Vec<Option<u32>>,
    pub pending_new_spawn: Option<PendingNewSpawn>,
}
