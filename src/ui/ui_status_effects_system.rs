use std::time::Duration;

use bevy::{
    ecs::query::WorldQuery,
    prelude::{Entity, Query, Res, With},
    time::Time,
};
use bevy_egui::{egui, EguiContexts};

use rose_game_common::components::StatusEffects;

use crate::{
    components::{PlayerCharacter, SummonPoints},
    resources::{GameData, UiResources, UiSpriteSheetType},
};

const ENDU_ICON_X: f32 = 250.0;
const ENDU_ICON_Y: f32 = 40.0;
const SUMMON_GAUGE_Y_OFFSET: f32 = 65.0;
const SUMMON_GAUGE_WIDTH: f32 = 100.0;
const SUMMON_GAUGE_HEIGHT: f32 = 20.0;
const SUMMON_GAUGE_BG_SPRITE: &str = "UI00_GUAGE_BACKGROUND";
const SUMMON_GAUGE_FG_SPRITE: &str = "UI00_GUAGE_VIOLET";

#[derive(WorldQuery)]
pub struct PlayerQuery<'w> {
    entity: Entity,
    status_effects: &'w StatusEffects,
    summon_points: Option<&'w SummonPoints>,
}

fn draw_summon_gauge(
    ui: &mut egui::Ui,
    ui_resources: &UiResources,
    summon_percent: f32,
    summon_text: &str,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(SUMMON_GAUGE_WIDTH, SUMMON_GAUGE_HEIGHT),
        egui::Sense::hover(),
    );

    if ui.is_rect_visible(rect) {
        if let Some(sprite) = ui_resources.get_sprite(0, SUMMON_GAUGE_BG_SPRITE) {
            sprite.draw_stretched(ui, rect);
        }

        let summon_percent = summon_percent.clamp(0.0, 1.0);
        if summon_percent * rect.width() > 0.5 {
            if let Some(sprite) = ui_resources.get_sprite(0, SUMMON_GAUGE_FG_SPRITE) {
                let mut filled_rect = rect;
                filled_rect.set_width(rect.width() * summon_percent);
                sprite.draw_stretched(ui, filled_rect);
            }
        }

        let text_pos = rect.center();
        let font_id = egui::FontId::new(12.0, egui::FontFamily::Proportional);
        ui.painter().text(
            text_pos + egui::vec2(1.0, 1.0),
            egui::Align2::CENTER_CENTER,
            summon_text,
            font_id.clone(),
            egui::Color32::BLACK,
        );
        ui.painter().text(
            text_pos,
            egui::Align2::CENTER_CENTER,
            summon_text,
            font_id,
            egui::Color32::WHITE,
        );
    }

    response
}

pub fn ui_status_effects_system(
    mut egui_context: EguiContexts,
    query_player: Query<PlayerQuery, With<PlayerCharacter>>,
    game_data: Res<GameData>,
    ui_resources: Res<UiResources>,
    time: Res<Time>,
) {
    let player = if let Ok(player) = query_player.get_single() {
        player
    } else {
        return;
    };

    let summon_max = player
        .summon_points
        .map_or(0, |summon_points| summon_points.max_points);
    let summon_used = player
        .summon_points
        .map_or(0, |summon_points| summon_points.used_points);
    let summon_gauge_visible = summon_max > 0 && summon_used > 0;

    egui::Window::new("Player Status Effects")
        .anchor(egui::Align2::LEFT_TOP, [ENDU_ICON_X, ENDU_ICON_Y])
        .frame(egui::Frame::none())
        .title_bar(false)
        .resizable(false)
        .show(egui_context.ctx_mut(), |ui| {
            ui.horizontal_top(|ui| {
                for (status_effect_type, active_status_effect) in
                    player.status_effects.active.iter()
                {
                    if let Some(active_status_effect) = active_status_effect {
                        if let Some(status_effect_data) = game_data
                            .status_effects
                            .get_status_effect(active_status_effect.id)
                        {
                            let remaining_time = if let Some(expire_time) =
                                player.status_effects.expire_times[status_effect_type]
                            {
                                let now = time.last_update().unwrap();
                                if now >= expire_time {
                                    Some(Duration::ZERO)
                                } else {
                                    Some(expire_time - now)
                                }
                            } else {
                                None
                            };

                            if let Some(sprite) = ui_resources.get_sprite_by_index(
                                UiSpriteSheetType::StateIcon,
                                status_effect_data.icon_id as usize,
                            ) {
                                let (rect, response) = ui.allocate_exact_size(
                                    egui::vec2(sprite.width, sprite.height),
                                    egui::Sense::hover(),
                                );
                                sprite.draw(ui, rect.min);

                                if response.hovered() {
                                    if let Some(remaining_time) = remaining_time {
                                        response.on_hover_text(format!(
                                            "{}\n\nTime Remaining: {} seconds",
                                            status_effect_data.name,
                                            remaining_time.as_secs()
                                        ));
                                    } else {
                                        response.on_hover_text(status_effect_data.name);
                                    }
                                }
                            }
                        }
                    }
                }
            });
        });

    if summon_gauge_visible {
        let summon_percent = summon_used as f32 / summon_max as f32;
        let summon_text = format!("{} / {}", summon_used, summon_max);
        let summon_tooltip_text = format!(
            "{}: {} / {}",
            game_data.client_strings.skill_summon_point_cost, summon_used, summon_max
        );
        let response = egui::Window::new("Player Summon Gauge")
            .anchor(
                egui::Align2::LEFT_TOP,
                [ENDU_ICON_X, ENDU_ICON_Y + SUMMON_GAUGE_Y_OFFSET],
            )
            .frame(egui::Frame::none())
            .title_bar(false)
            .resizable(false)
            .show(egui_context.ctx_mut(), |ui| {
                draw_summon_gauge(ui, &ui_resources, summon_percent, &summon_text)
            });

        if let Some(response) = response {
            if let Some(gauge_response) = response.inner {
                gauge_response.on_hover_text(summon_tooltip_text);
            }
        }
    }
}
