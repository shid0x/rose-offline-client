use bevy::{
    ecs::query::WorldQuery,
    prelude::{Assets, EventWriter, Local, Query, Res, ResMut, With},
};
use bevy_egui::{egui, EguiContexts};

use rose_data::{AbilityType, SkillId};
use rose_data_irose::{IroseSkillPageType, SKILL_PAGE_SIZE};
use rose_game_common::components::{CharacterInfo, SkillList, SkillPoints, SkillSlot};
use rose_game_common::messages::client::ClientMessage;

use crate::{
    bundles::ability_values_get_value,
    components::{Cooldowns, PlayerCharacter},
    events::PlayerCommandEvent,
    resources::{GameConnection, GameData, UiResources},
    ui::{
        tooltips::{PlayerTooltipQuery, PlayerTooltipQueryItem, SkillTooltipType},
        ui_add_skill_tooltip,
        widgets::{DataBindings, Dialog, DrawText, Widget},
        DragAndDropId, DragAndDropSlot, UiSoundEvent, UiStateDragAndDrop, UiStateWindows,
    },
};

const IID_BTN_CLOSE: i32 = 10;
// const IID_BTN_ICONIZE: i32 = 11;
const IID_BTN_OPEN_SKILLTREE: i32 = 12;
const IID_TABBEDPANE: i32 = 20;

const IID_TAB_BASIC: i32 = 21;
// const IID_BTN_BASIC: i32 = 25;
const IID_ZLISTBOX_BASIC: i32 = 26;
// const IID_SCROLLBAR_BASIC: i32 = 27;

const IID_TAB_ACTIVE: i32 = 31;
// const IID_BTN_ACTIVE: i32 = 35;
const IID_ZLISTBOX_ACTIVE: i32 = 36;
// const IID_SCROLLBAR_ACTIVE: i32 = 37;

const IID_TAB_PASSIVE: i32 = 41;
// const IID_BTN_PASSIVE: i32 = 45;
const IID_ZLISTBOX_PASSIVE: i32 = 46;
// const IID_SCROLLBAR_PASSIVE: i32 = 47;
const DEBUG_SKILL_UP_RECT_OVERLAY: bool = false;
const SKILL_ROW_PLUS_OFFSET_X: f32 = 190.0;
const SKILL_ROW_PLUS_OFFSET_Y: f32 = 8.0;
// Calibration offsets for the skill '+' icon rect after XML-derived placement.
// These values are applied to the final rect used by drawing, hit-testing, and tooltip anchoring.
// Negative X moves left, positive Y moves down.
const PLUS_NUDGE_X: f32 = -19.0;
const PLUS_NUDGE_Y: f32 = 17.0;

pub struct UiStateSkillList {
    current_page: i32,
    scroll_index_basic: i32,
    scroll_index_active: i32,
    scroll_index_passive: i32,
}

impl Default for UiStateSkillList {
    fn default() -> Self {
        Self {
            current_page: IID_TAB_BASIC,
            scroll_index_basic: 0,
            scroll_index_active: 0,
            scroll_index_passive: 0,
        }
    }
}

struct SkillUpButtonLayout {
    row_x: f32,
    row_y: f32,
    row_width: f32,
    row_height: f32,
    row_step: f32,
    plus_offset_x: f32,
    plus_offset_y: f32,
    plus_width: f32,
    plus_height: f32,
}

fn can_level_up_skill_now(
    game_data: &GameData,
    player: &PlayerQueryItem,
    player_tooltip_data: Option<&PlayerTooltipQueryItem>,
    skill_slot: SkillSlot,
    current_skill_id: SkillId,
) -> Result<SkillId, &'static str> {
    let current_skill_data = game_data
        .skills
        .get_skill(current_skill_id)
        .ok_or("No current skill data")?;
    let next_skill_id = SkillId::new(current_skill_id.get() + 1).ok_or("At max level")?;
    let next_skill_data = game_data
        .skills
        .get_skill(next_skill_id)
        .ok_or("At max level")?;

    if next_skill_data.base_skill_id != current_skill_data.base_skill_id
        || next_skill_data.level != current_skill_data.level + 1
    {
        return Err("At max level");
    }

    if player.skill_points.points < next_skill_data.learn_point_cost {
        return Err("Not enough skill points");
    }

    if let Some(job_class_id) = next_skill_data.required_job_class {
        if let Some(job_class) = game_data.job_class.get(job_class_id) {
            if !job_class
                .jobs
                .contains(&rose_data::JobId::new(player.character_info.job))
            {
                return Err("Job requirement not met");
            }
        }
    }

    for &(required_skill_id, required_level) in next_skill_data.required_skills.iter() {
        if let Some(required_skill_data) = game_data.skills.get_skill(
            SkillId::new(required_skill_id.get() + required_level.max(1) as u16 - 1).unwrap(),
        ) {
            let Some((_, _, skill_level)) = player.skill_list.find_skill_level(
                &game_data.skills,
                required_skill_data
                    .base_skill_id
                    .unwrap_or(required_skill_id),
            ) else {
                return Err("Skill requirement not met");
            };

            if skill_level < required_level as u32 {
                return Err("Skill requirement not met");
            }
        }
    }

    if let Some(player_tooltip_data) = player_tooltip_data {
        for &(ability_type, required_value) in next_skill_data.required_ability.iter() {
            let Some(current_value) = ability_values_get_value(
                ability_type,
                player_tooltip_data.ability_values,
                Some(player_tooltip_data.character_info),
                Some(player_tooltip_data.experience_points),
                Some(player_tooltip_data.health_points),
                Some(player_tooltip_data.inventory),
                Some(player_tooltip_data.level),
                Some(player_tooltip_data.mana_points),
                Some(player_tooltip_data.move_speed),
                Some(player_tooltip_data.skill_points),
                Some(player_tooltip_data.stamina),
                Some(player_tooltip_data.stat_points),
                Some(player_tooltip_data.team),
                Some(player_tooltip_data.union_membership),
            ) else {
                return Err("Ability requirement not met");
            };

            if current_value < required_value {
                return Err("Ability requirement not met");
            }
        }
    }

    if player.skill_list.get_skill(skill_slot) != Some(current_skill_id) {
        return Err("Invalid skill slot");
    }

    Ok(next_skill_id)
}

fn parse_skill_up_button_layout(
    dialog: &Dialog,
    ui_resources: &UiResources,
) -> Option<SkillUpButtonLayout> {
    let plus_normal = ui_resources.get_sprite(0, "UI09_BTN_PLUS_NORMAL")?;

    let tabbed_pane = match dialog.get_widget(IID_TABBEDPANE)? {
        Widget::TabbedPane(tabbed_pane) => tabbed_pane,
        _ => return None,
    };

    let tab = tabbed_pane
        .tabs
        .iter()
        .find(|tab| tab.id == IID_TAB_BASIC)
        .or_else(|| tabbed_pane.tabs.first())?;

    let mut list_offset_y = None;
    let mut row_offsets = Vec::new();
    let mut row_offset_x = None;
    let mut row_width = None;
    let mut row_height = None;

    for widget in &tab.widgets {
        match widget {
            Widget::ZListbox(zlistbox) if zlistbox.id == IID_ZLISTBOX_BASIC => {
                list_offset_y = Some(zlistbox.offset_y);
            }
            Widget::Image(image) if image.sprite_name == "UI09_MIDDLE" => {
                row_offsets.push(image.offset_y);
                row_offset_x = Some(image.offset_x);
                row_width = Some(image.width);
                row_height = Some(image.height);
            }
            _ => {}
        }
    }

    let mut row_step = 44.0;
    row_offsets.sort_by(f32::total_cmp);
    for pair in row_offsets.windows(2) {
        let delta = pair[1] - pair[0];
        if delta > 0.0 {
            row_step = delta;
            break;
        }
    }

    let row_offset_y = list_offset_y.or_else(|| row_offsets.first().copied())?;
    let row_offset_x = row_offset_x.unwrap_or(0.0);
    let row_width = row_width.unwrap_or(dialog.width);
    let row_height = row_height.unwrap_or(45.0);

    Some(SkillUpButtonLayout {
        row_x: tabbed_pane.x + row_offset_x,
        row_y: tabbed_pane.y + row_offset_y,
        row_width,
        row_height,
        row_step,
        plus_offset_x: SKILL_ROW_PLUS_OFFSET_X,
        plus_offset_y: SKILL_ROW_PLUS_OFFSET_Y,
        plus_width: plus_normal.width,
        plus_height: plus_normal.height,
    })
}

fn ui_add_skill_list_slot(
    ui: &mut egui::Ui,
    pos: egui::Pos2,
    skill_slot: SkillSlot,
    player: &PlayerQueryItem,
    player_tooltip_data: Option<&PlayerTooltipQueryItem>,
    game_data: &GameData,
    ui_resources: &UiResources,
    ui_state_dnd: &mut UiStateDragAndDrop,
    player_command_events: &mut EventWriter<PlayerCommandEvent>,
) {
    let skill = player.skill_list.get_skill(skill_slot);
    let mut dropped_item = None;
    let response = ui
        .allocate_ui_at_rect(
            egui::Rect::from_min_size(pos, egui::vec2(40.0, 40.0)),
            |ui| {
                egui::Widget::ui(
                    DragAndDropSlot::with_skill(
                        DragAndDropId::Skill(skill_slot),
                        skill.as_ref(),
                        Some(player.cooldowns),
                        game_data,
                        ui_resources,
                        |_| false,
                        &mut ui_state_dnd.dragged_item,
                        &mut dropped_item,
                        [40.0, 40.0],
                    ),
                    ui,
                )
            },
        )
        .inner;

    if response.double_clicked() {
        player_command_events.send(PlayerCommandEvent::UseSkill(skill_slot));
    }

    if let Some(skill_id) = skill {
        response.on_hover_ui(|ui| {
            let extra = ui.input(|input| input.pointer.secondary_down());
            ui_add_skill_tooltip(
                ui,
                if extra {
                    SkillTooltipType::Extra
                } else {
                    SkillTooltipType::Detailed
                },
                game_data,
                player_tooltip_data,
                skill_id,
            );
        });
    }
}

#[derive(WorldQuery)]
pub struct PlayerQuery<'w> {
    character_info: &'w CharacterInfo,
    skill_list: &'w SkillList,
    skill_points: &'w SkillPoints,
    cooldowns: &'w Cooldowns,
}

pub fn ui_skill_list_system(
    mut egui_context: EguiContexts,
    mut ui_state_skill_list: Local<UiStateSkillList>,
    mut ui_state_dnd: ResMut<UiStateDragAndDrop>,
    mut ui_state_windows: ResMut<UiStateWindows>,
    mut ui_sound_events: EventWriter<UiSoundEvent>,
    mut player_command_events: EventWriter<PlayerCommandEvent>,
    query_player: Query<PlayerQuery, With<PlayerCharacter>>,
    query_player_tooltip: Query<PlayerTooltipQuery, With<PlayerCharacter>>,
    game_data: Res<GameData>,
    ui_resources: Res<UiResources>,
    dialog_assets: Res<Assets<Dialog>>,
    game_connection: Option<Res<GameConnection>>,
) {
    let ui_state_skill_list = &mut *ui_state_skill_list;
    let dialog = if let Some(dialog) = dialog_assets.get(&ui_resources.dialog_skill_list) {
        dialog
    } else {
        return;
    };

    let player = if let Ok(skill_list) = query_player.get_single() {
        skill_list
    } else {
        return;
    };
    let player_tooltip_data = query_player_tooltip.get_single().ok();
    let skill_up_layout = parse_skill_up_button_layout(dialog, &ui_resources);
    let plus_normal_sprite = ui_resources.get_sprite(0, "UI09_BTN_PLUS_NORMAL");
    let plus_over_sprite = ui_resources.get_sprite(0, "UI09_BTN_PLUS_OVER");
    let plus_down_sprite = ui_resources.get_sprite(0, "UI09_BTN_PLUS_DOWN");
    let plus_disable_sprite = ui_resources.get_sprite(0, "UI09_BTN_PLUS_DISABLE");

    let listbox_extent =
        if let Some(Widget::ZListbox(listbox)) = dialog.get_widget(IID_ZLISTBOX_BASIC) {
            listbox.extent
        } else {
            1
        };
    let scrollbar_range = 0..SKILL_PAGE_SIZE as i32;

    let mut response_close_button = None;
    let mut response_skill_tree_button = None;
    let mut debug_content_rect: Option<egui::Rect> = None;
    let mut debug_plus_base_rect: Option<egui::Rect> = None;
    let mut debug_plus_nudged_rect: Option<egui::Rect> = None;
    let mut debug_anchor: Option<egui::Pos2> = None;
    let mut debug_plus_rows: Vec<(egui::Rect, bool)> = Vec::new();

    let window_response = egui::Window::new("Skills")
        .frame(egui::Frame::none())
        .open(&mut ui_state_windows.skill_list_open)
        .title_bar(false)
        .resizable(false)
        .default_width(dialog.width)
        .default_height(dialog.height)
        .show(egui_context.ctx_mut(), |ui| {
            // DLGSKILL widget coordinates are dialog-local. Convert to screen-space by using
            // the actual content area origin of this egui window.
            let dialog_screen_origin = ui.max_rect().min;
            let dialog_content_rect =
                egui::Rect::from_min_size(dialog_screen_origin, egui::vec2(dialog.width, dialog.height));
            if DEBUG_SKILL_UP_RECT_OVERLAY {
                debug_content_rect = Some(dialog_content_rect);
            }

            dialog.draw(
                ui,
                DataBindings {
                    sound_events: Some(&mut ui_sound_events),
                    tabs: &mut [(IID_TABBEDPANE, &mut ui_state_skill_list.current_page)],
                    scroll: &mut [
                        (
                            IID_ZLISTBOX_BASIC,
                            (
                                &mut ui_state_skill_list.scroll_index_basic,
                                scrollbar_range.clone(),
                                listbox_extent,
                            ),
                        ),
                        (
                            IID_ZLISTBOX_ACTIVE,
                            (
                                &mut ui_state_skill_list.scroll_index_active,
                                scrollbar_range.clone(),
                                listbox_extent,
                            ),
                        ),
                        (
                            IID_ZLISTBOX_PASSIVE,
                            (
                                &mut ui_state_skill_list.scroll_index_passive,
                                scrollbar_range.clone(),
                                listbox_extent,
                            ),
                        ),
                    ],
                    visible: &mut [(IID_BTN_OPEN_SKILLTREE, player.character_info.job != 0)],
                    label: &mut [(IID_BTN_OPEN_SKILLTREE, "Skill Tree")],
                    response: &mut [
                        (IID_BTN_CLOSE, &mut response_close_button),
                        (IID_BTN_OPEN_SKILLTREE, &mut response_skill_tree_button),
                    ],
                    ..Default::default()
                },
                |ui, bindings| {
                    let (page, index) = match bindings.get_tab(IID_TABBEDPANE) {
                        Some(&mut IID_TAB_BASIC) => (
                            IroseSkillPageType::Basic,
                            bindings.get_scroll(IID_ZLISTBOX_BASIC).map_or(0, |s| *s.0),
                        ),
                        Some(&mut IID_TAB_ACTIVE) => (
                            IroseSkillPageType::Active,
                            bindings.get_scroll(IID_ZLISTBOX_ACTIVE).map_or(0, |s| *s.0),
                        ),
                        Some(&mut IID_TAB_PASSIVE) => (
                            IroseSkillPageType::Passive,
                            bindings
                                .get_scroll(IID_ZLISTBOX_PASSIVE)
                                .map_or(0, |s| *s.0),
                        ),
                        _ => (IroseSkillPageType::Basic, 0),
                    };

                    let listbox_pos = egui::vec2(0.0, 65.0);
                    for i in 0..listbox_extent {
                        let skill_slot = SkillSlot(page as usize, (index + i) as usize);
                        let start_x = listbox_pos.x + 16.0;
                        let start_y = listbox_pos.y + 44.0 * i as f32;

                        let skill = player.skill_list.get_skill(skill_slot);
                        let skill_data = skill
                            .as_ref()
                            .and_then(|skill| game_data.skills.get_skill(*skill));
                        if let Some(skill_data) = skill_data {
                            // Skill name
                            if skill_data.level > 0 {
                                ui.add_label_at(
                                    egui::pos2(start_x + 46.0, start_y + 5.0),
                                    format!("{} (Lv: {})", skill_data.name, skill_data.level),
                                );
                            } else {
                                ui.add_label_at(
                                    egui::pos2(start_x + 46.0, start_y + 5.0),
                                    skill_data.name,
                                );
                            }

                            // Skill use ability values
                            if !skill_data.use_ability.is_empty() {
                                ui.allocate_ui_at_rect(
                                    egui::Rect::from_min_size(
                                        ui.min_rect().min
                                            + egui::vec2(start_x + 46.0, start_y + 25.0),
                                        egui::vec2(100.0, 18.0),
                                    ),
                                    |ui| {
                                        ui.horizontal(|ui| {
                                            for &(ability_type, mut value) in
                                                skill_data.use_ability.iter()
                                            {
                                                let mut color = egui::Color32::RED;

                                                if let Some(player_tooltip_data) =
                                                    player_tooltip_data.as_ref()
                                                {
                                                    if matches!(ability_type, AbilityType::Mana) {
                                                        let use_mana_rate = (100
                                                            - player_tooltip_data
                                                                .ability_values
                                                                .get_save_mana())
                                                            as f32
                                                            / 100.0;
                                                        value =
                                                            (value as f32 * use_mana_rate) as i32;
                                                    }

                                                    if let Some(current_value) =
                                                        ability_values_get_value(
                                                            ability_type,
                                                            player_tooltip_data.ability_values,
                                                            Some(
                                                                player_tooltip_data.character_info,
                                                            ),
                                                            Some(
                                                                player_tooltip_data
                                                                    .experience_points,
                                                            ),
                                                            Some(player_tooltip_data.health_points),
                                                            Some(player_tooltip_data.inventory),
                                                            Some(player_tooltip_data.level),
                                                            Some(player_tooltip_data.mana_points),
                                                            Some(player_tooltip_data.move_speed),
                                                            Some(player_tooltip_data.skill_points),
                                                            Some(player_tooltip_data.stamina),
                                                            Some(player_tooltip_data.stat_points),
                                                            Some(player_tooltip_data.team),
                                                            Some(
                                                                player_tooltip_data
                                                                    .union_membership,
                                                            ),
                                                        )
                                                    {
                                                        if current_value >= value {
                                                            color = egui::Color32::GREEN;
                                                        }
                                                    }
                                                }

                                                ui.colored_label(
                                                    color,
                                                    format!(
                                                        "{} {}",
                                                        game_data
                                                            .string_database
                                                            .get_ability_type(ability_type),
                                                        value
                                                    ),
                                                );
                                            }
                                        });
                                    },
                                );
                            }
                        }

                        if let Some(current_skill_id) = skill {
                            let can_level_up_result = can_level_up_skill_now(
                                &game_data,
                                &player,
                                player_tooltip_data.as_ref(),
                                skill_slot,
                                current_skill_id,
                            );
                            let can_level_up = can_level_up_result.is_ok();
                            let disabled_reason = can_level_up_result.err();

                            let (row_x, row_y, _row_width, _row_height, row_step, plus_offset_x, plus_offset_y, plus_w, plus_h) =
                                if let Some(layout) = skill_up_layout.as_ref() {
                                    (
                                        layout.row_x,
                                        layout.row_y,
                                        layout.row_width,
                                        layout.row_height,
                                        layout.row_step,
                                        layout.plus_offset_x,
                                        layout.plus_offset_y,
                                        layout.plus_width,
                                        layout.plus_height,
                                    )
                                } else {
                                    (
                                        0.0,
                                        listbox_pos.y,
                                        223.0,
                                        45.0,
                                        44.0,
                                        SKILL_ROW_PLUS_OFFSET_X,
                                        SKILL_ROW_PLUS_OFFSET_Y,
                                        16.0,
                                        16.0,
                                    )
                                };
                            let row_min = dialog_screen_origin + egui::vec2(row_x, row_y + i as f32 * row_step);
                            let plus_base_rect = egui::Rect::from_min_size(
                                row_min + egui::vec2(plus_offset_x, plus_offset_y),
                                egui::vec2(plus_w, plus_h),
                            );
                            let up_rect =
                                plus_base_rect.translate(egui::vec2(PLUS_NUDGE_X, PLUS_NUDGE_Y));

                            let mut response_upgrade_button = ui.allocate_rect(
                                up_rect,
                                if can_level_up {
                                    egui::Sense::click()
                                } else {
                                    egui::Sense::hover()
                                },
                            );

                            if DEBUG_SKILL_UP_RECT_OVERLAY && debug_plus_nudged_rect.is_none() {
                                debug_plus_base_rect = Some(plus_base_rect);
                                debug_plus_nudged_rect = Some(up_rect);
                                debug_anchor = Some(up_rect.center());
                            }
                            if DEBUG_SKILL_UP_RECT_OVERLAY {
                                debug_plus_rows.push((up_rect, can_level_up));
                            }

                            let sprite_to_draw = if can_level_up {
                                if response_upgrade_button.is_pointer_button_down_on() {
                                    plus_down_sprite
                                        .as_ref()
                                        .or(plus_over_sprite.as_ref())
                                        .or(plus_normal_sprite.as_ref())
                                } else if response_upgrade_button.hovered() {
                                    plus_over_sprite
                                        .as_ref()
                                        .or(plus_normal_sprite.as_ref())
                                } else {
                                    plus_normal_sprite.as_ref()
                                }
                            } else {
                                plus_disable_sprite.as_ref()
                            };

                            if let Some(sprite) = sprite_to_draw {
                                sprite.draw(ui, up_rect.min);
                            } else if response_upgrade_button.hovered() && can_level_up {
                                ui.painter().rect_filled(
                                    up_rect,
                                    1.0,
                                    egui::Color32::from_rgba_premultiplied(40, 180, 40, 120),
                                );
                            }

                            if can_level_up {
                                response_upgrade_button = response_upgrade_button.on_hover_text("Up");
                            } else if let Some(reason) = disabled_reason {
                                response_upgrade_button =
                                    response_upgrade_button.on_hover_text(reason);
                            }

                            if can_level_up && response_upgrade_button.clicked() {
                                if let Some(game_connection) = game_connection.as_ref() {
                                    game_connection
                                        .client_message_tx
                                        .send(ClientMessage::LevelUpSkill { skill_slot })
                                        .ok();
                                }
                            }
                        }

                        ui_add_skill_list_slot(
                            ui,
                            dialog_screen_origin + egui::vec2(start_x, start_y + 3.0),
                            skill_slot,
                            &player,
                            player_tooltip_data.as_ref(),
                            &game_data,
                            &ui_resources,
                            &mut ui_state_dnd,
                            &mut player_command_events,
                        );
                    }

                    ui.add_label_at(
                        egui::pos2(40.0, dialog.height - 25.0),
                        &format!("{}", player.skill_points.points),
                    );
                },
            );
        });

    if DEBUG_SKILL_UP_RECT_OVERLAY {
        let debug_painter = egui_context.ctx_mut().debug_painter();
        if let Some(window_rect) = window_response.as_ref().map(|r| r.response.rect) {
            debug_painter.rect_stroke(
                window_rect,
                0.0,
                egui::Stroke::new(1.0, egui::Color32::YELLOW),
            );
        }
        if let Some(content_rect) = debug_content_rect {
            debug_painter.rect_stroke(
                content_rect,
                0.0,
                egui::Stroke::new(1.0, egui::Color32::LIGHT_BLUE),
            );
        }
        if let Some(base_rect) = debug_plus_base_rect {
            debug_painter.rect_stroke(
                base_rect,
                0.0,
                egui::Stroke::new(1.0, egui::Color32::from_rgb(180, 60, 255)),
            );
        }
        if let Some(nudged_rect) = debug_plus_nudged_rect {
            debug_painter.rect_stroke(
                nudged_rect,
                0.0,
                egui::Stroke::new(1.0, egui::Color32::GREEN),
            );
        }
        if let Some(anchor) = debug_anchor {
            debug_painter.circle_filled(anchor, 2.5, egui::Color32::RED);
        }
        for (index, (row_rect, upgradable)) in debug_plus_rows.iter().enumerate() {
            debug_painter.text(
                row_rect.right_top() + egui::vec2(3.0, 0.0),
                egui::Align2::LEFT_TOP,
                format!("row {}: {}", index, if *upgradable { "UP" } else { "NO" }),
                egui::FontId::monospace(10.0),
                if *upgradable {
                    egui::Color32::GREEN
                } else {
                    egui::Color32::GRAY
                },
            );
        }
    }

    if response_skill_tree_button.map_or(false, |r| r.clicked()) {
        ui_state_windows.skill_tree_open = !ui_state_windows.skill_tree_open;
    }

    if response_close_button.map_or(false, |r| r.clicked()) {
        ui_state_windows.skill_list_open = false;
    }
}
