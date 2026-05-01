use bevy::prelude::{Assets, Color, Gizmos, Res, State};

use crate::{
    resources::{AppState, CurrentZone, SpawnEditorState},
    ui::UiStateDebugWindows,
    zone_loader::ZoneLoaderAsset,
};

pub fn debug_render_spawn_system(
    ui_state_debug_windows: Res<UiStateDebugWindows>,
    spawn_editor_state: Res<SpawnEditorState>,
    app_state: Res<State<AppState>>,
    current_zone: Option<Res<CurrentZone>>,
    zone_loader_assets: Res<Assets<ZoneLoaderAsset>>,
    mut gizmos: Gizmos,
) {
    if !ui_state_debug_windows.debug_ui_open {
        return;
    }
    if matches!(app_state.get(), AppState::Game) && !spawn_editor_state.active {
        return;
    }

    let Some(current_zone) = current_zone else {
        return;
    };
    let Some(zone_asset) = zone_loader_assets.get(&current_zone.handle) else {
        return;
    };

    for (index, spawn) in zone_asset.monster_spawns.iter().enumerate() {
        let selected = spawn_editor_state.selected_spawn == Some(index);
        let color = if selected { Color::YELLOW } else { Color::CYAN };
        let center = bevy::prelude::Vec3::new(
            spawn.position.x / 100.0,
            spawn.position.z / 100.0,
            -spawn.position.y / 100.0,
        );
        let marker_size = if selected { 2.0 } else { 1.0 };

        gizmos.line(
            center + bevy::prelude::Vec3::X * marker_size,
            center - bevy::prelude::Vec3::X * marker_size,
            color,
        );
        gizmos.line(
            center + bevy::prelude::Vec3::Z * marker_size,
            center - bevy::prelude::Vec3::Z * marker_size,
            color,
        );
        gizmos.line(
            center + bevy::prelude::Vec3::Y * marker_size,
            center - bevy::prelude::Vec3::Y * marker_size,
            color,
        );

        draw_ground_circle(&mut gizmos, center, spawn.range as f32, color);
    }
}

fn draw_ground_circle(gizmos: &mut Gizmos, center: bevy::prelude::Vec3, radius: f32, color: Color) {
    if radius <= 0.0 {
        return;
    }

    const SEGMENTS: usize = 48;
    let mut previous = center + bevy::prelude::Vec3::X * radius;
    for segment in 1..=SEGMENTS {
        let angle = segment as f32 / SEGMENTS as f32 * std::f32::consts::TAU;
        let next =
            center + bevy::prelude::Vec3::new(angle.cos() * radius, 0.0, angle.sin() * radius);
        gizmos.line(previous, next, color);
        previous = next;
    }
}
