use bevy::prelude::{Entity, EventReader, Local, Query, Res, ResMut, With};
use bevy_egui::{egui, EguiContexts};
use egui_extras::{Column, TableBuilder};
use rose_data::ClanMemberPosition;
use rose_game_common::messages::client::ClientMessage;

use crate::{
    components::{
        Clan, ClanMembership, ClientEntity, ClientEntityName, ClientEntityType, PlayerCharacter,
    },
    events::ClanDialogEvent,
    resources::{GameConnection, GameData, SelectedTarget},
    ui::UiStateWindows,
};

#[derive(Copy, Clone, Eq, PartialEq)]
enum ClanTab {
    Info,
    Members,
}

#[derive(Clone)]
enum ClanActionConfirm {
    Leave,
    Expel { name: String },
    Promote {
        name: String,
        next_position_label: String,
    },
    Demote {
        name: String,
        next_position_label: String,
    },
}

pub struct UiStateClan {
    active_tab: ClanTab,
    last_window_size: Option<egui::Vec2>,
    was_open: bool,
    had_clan_last_frame: bool,
    has_centered_once: bool,
    is_editing_slogan: bool,
    slogan_edit_buffer: String,
    selected_member_name: Option<String>,
    pending_action_confirm: Option<ClanActionConfirm>,
}

impl Default for UiStateClan {
    fn default() -> Self {
        Self {
            active_tab: ClanTab::Info,
            last_window_size: None,
            was_open: false,
            had_clan_last_frame: false,
            has_centered_once: false,
            is_editing_slogan: false,
            slogan_edit_buffer: String::new(),
            selected_member_name: None,
            pending_action_confirm: None,
        }
    }
}

fn format_number_with_commas(value: u64) -> String {
    let value = value.to_string();
    let mut formatted = String::with_capacity(value.len() + value.len() / 3);

    for (index, ch) in value.chars().enumerate() {
        if index > 0 && (value.len() - index) % 3 == 0 {
            formatted.push(',');
        }

        formatted.push(ch);
    }

    formatted
}

fn position_to_rank(position: ClanMemberPosition) -> u8 {
    match position {
        ClanMemberPosition::Penalty => 0,
        ClanMemberPosition::Junior => 1,
        ClanMemberPosition::Senior => 2,
        ClanMemberPosition::Veteran => 3,
        ClanMemberPosition::Commander => 4,
        ClanMemberPosition::DeputyMaster => 5,
        ClanMemberPosition::Master => 6,
    }
}

fn rank_to_position(rank: u8) -> Option<ClanMemberPosition> {
    match rank {
        0 => Some(ClanMemberPosition::Penalty),
        1 => Some(ClanMemberPosition::Junior),
        2 => Some(ClanMemberPosition::Senior),
        3 => Some(ClanMemberPosition::Veteran),
        4 => Some(ClanMemberPosition::Commander),
        5 => Some(ClanMemberPosition::DeputyMaster),
        6 => Some(ClanMemberPosition::Master),
        _ => None,
    }
}

fn next_promoted_position(
    actor_position: ClanMemberPosition,
    target_position: ClanMemberPosition,
) -> Option<ClanMemberPosition> {
    let actor_rank = position_to_rank(actor_position);
    let target_rank = position_to_rank(target_position);
    let promoted_rank = target_rank.checked_add(1)?;
    if target_rank >= actor_rank || promoted_rank >= actor_rank {
        return None;
    }

    rank_to_position(promoted_rank)
}

fn next_demoted_position(
    actor_position: ClanMemberPosition,
    target_position: ClanMemberPosition,
) -> Option<ClanMemberPosition> {
    let actor_rank = position_to_rank(actor_position);
    let target_rank = position_to_rank(target_position);
    if target_rank >= actor_rank {
        return None;
    }

    let demoted_rank = target_rank.checked_sub(1)?;
    rank_to_position(demoted_rank)
}

fn clan_position_name(game_data: &GameData, position: ClanMemberPosition) -> String {
    let name = game_data
        .string_database
        .get_clan_member_position(position)
        .trim();
    if !name.is_empty() {
        return name.to_string();
    }

    match position {
        ClanMemberPosition::Penalty => "Penalty".to_string(),
        ClanMemberPosition::Junior => "Junior".to_string(),
        ClanMemberPosition::Senior => "Senior".to_string(),
        ClanMemberPosition::Veteran => "Veteran".to_string(),
        ClanMemberPosition::Commander => "Commander".to_string(),
        ClanMemberPosition::DeputyMaster => "Deputy Master".to_string(),
        ClanMemberPosition::Master => "Master".to_string(),
    }
}

fn draw_tab_button(ui: &mut egui::Ui, text: &str, is_active: bool) -> egui::Response {
    let fill = if is_active {
        egui::Color32::from_rgb(104, 38, 20)
    } else {
        egui::Color32::from_rgb(32, 32, 36)
    };
    let stroke_color = if is_active {
        egui::Color32::from_rgb(190, 92, 52)
    } else {
        egui::Color32::from_rgb(74, 74, 74)
    };
    let text_color = if is_active {
        egui::Color32::from_rgb(255, 232, 156)
    } else {
        egui::Color32::from_rgb(224, 224, 224)
    };

    ui.add_sized(
        [120.0, 28.0],
        egui::Button::new(egui::RichText::new(text).color(text_color).strong())
            .fill(fill)
            .stroke(egui::Stroke::new(1.0, stroke_color))
            .rounding(egui::Rounding::same(2.0)),
    )
}

fn draw_clan_info_tab(
    ui: &mut egui::Ui,
    clan: &Clan,
    clan_membership: &ClanMembership,
    game_data: &GameData,
    ui_state: &mut UiStateClan,
    game_connection: Option<&GameConnection>,
) {
    let label_color = egui::Color32::from_rgb(214, 214, 214);
    let value_color = egui::Color32::from_rgb(240, 240, 240);
    let max_members = game_data
        .ability_value_calculator
        .calculate_clan_max_members(clan.level.0);
    let is_master = clan_membership.position == ClanMemberPosition::Master;

    if !ui_state.is_editing_slogan {
        ui_state.slogan_edit_buffer = clan.description.clone();
    } else if !is_master {
        ui_state.is_editing_slogan = false;
    }

    egui::Grid::new("clan_info_grid")
        .num_columns(2)
        .min_col_width(160.0)
        .spacing(egui::vec2(10.0, 8.0))
        .show(ui, |ui| {
            ui.colored_label(label_color, game_data.client_strings.clan_name);
            ui.colored_label(value_color, &clan.name);
            ui.end_row();

            ui.colored_label(label_color, "Clan Grade");
            ui.colored_label(value_color, format!("{}", clan.level.0.get()));
            ui.end_row();

            ui.colored_label(label_color, game_data.client_strings.clan_point);
            ui.colored_label(value_color, format_number_with_commas(clan.points.0));
            ui.end_row();

            ui.colored_label(label_color, game_data.client_strings.clan_member_count);
            ui.colored_label(
                value_color,
                format!("{} / {}", clan.members.len(), max_members),
            );
            ui.end_row();
        });

    ui.add_space(8.0);
    ui.colored_label(label_color, game_data.client_strings.clan_slogan);

    let slogan_panel_width = ui.available_width();
    if ui_state.is_editing_slogan && is_master {
        egui::Frame::none()
            .fill(egui::Color32::from_rgb(14, 14, 16))
            .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(92, 92, 92)))
            .inner_margin(egui::Margin::same(8.0))
            .show(ui, |ui| {
                ui.add_sized(
                    [(slogan_panel_width - 16.0).max(0.0), 92.0],
                    egui::TextEdit::multiline(&mut ui_state.slogan_edit_buffer)
                        .desired_rows(4)
                        .desired_width(f32::INFINITY),
                );
            });

        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui.button("Save").clicked() {
                if let Some(game_connection) = game_connection {
                    game_connection
                        .client_message_tx
                        .send(ClientMessage::ClanSetDescription {
                            description: ui_state.slogan_edit_buffer.clone(),
                        })
                        .ok();
                }
                ui_state.is_editing_slogan = false;
            }

            if ui.button("Cancel").clicked() {
                ui_state.slogan_edit_buffer = clan.description.clone();
                ui_state.is_editing_slogan = false;
            }
        });
    } else {
        let slogan = if clan.description.is_empty() {
            "-"
        } else {
            &clan.description
        };
        let response = egui::Frame::none()
            .fill(egui::Color32::from_rgb(14, 14, 16))
            .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(54, 54, 54)))
            .inner_margin(egui::Margin::same(8.0))
            .show(ui, |ui| {
                ui.add_sized(
                    [(slogan_panel_width - 16.0).max(0.0), 92.0],
                    egui::Label::new(egui::RichText::new(slogan).color(value_color))
                        .sense(if is_master {
                            egui::Sense::click()
                        } else {
                            egui::Sense::hover()
                        }),
                )
            })
            .inner;

        if is_master && response.clicked() {
            ui_state.slogan_edit_buffer = clan.description.clone();
            ui_state.is_editing_slogan = true;
        }
    }
}

fn resolve_invite_target(
    selected_target: &SelectedTarget,
    selected_target_query: &Query<(&ClientEntity, &ClientEntityName)>,
    player_entity: Option<Entity>,
) -> (Option<String>, String) {
    let Some(selected_entity) = selected_target.selected else {
        return (None, "No target selected.".to_string());
    };

    if Some(selected_entity) == player_entity {
        return (None, "You cannot invite yourself.".to_string());
    }

    let Ok((client_entity, client_entity_name)) = selected_target_query.get(selected_entity) else {
        return (None, "Invalid selected target.".to_string());
    };

    if client_entity.entity_type != ClientEntityType::Character {
        return (None, "Target must be a character.".to_string());
    }

    (Some(client_entity_name.name.clone()), String::new())
}

fn draw_clan_members_tab(
    ui: &mut egui::Ui,
    clan: &Clan,
    clan_membership: &ClanMembership,
    game_data: &GameData,
    ui_state: &mut UiStateClan,
    game_connection: Option<&GameConnection>,
    selected_target: &SelectedTarget,
    selected_target_query: &Query<(&ClientEntity, &ClientEntityName)>,
    player_entity: Option<Entity>,
    player_name: Option<&str>,
) {
    if ui_state
        .selected_member_name
        .as_ref()
        .map_or(false, |selected_name| clan.find_member(selected_name).is_none())
    {
        ui_state.selected_member_name = None;
    }

    if clan.members.is_empty() {
        ui.with_layout(
            egui::Layout::centered_and_justified(egui::Direction::TopDown),
            |ui| {
                ui.label(
                    egui::RichText::new("No members available.")
                        .color(egui::Color32::from_rgb(180, 180, 180)),
                );
            },
        );
    } else {
        let table_height = (ui.available_height() - 46.0).max(120.0);
        TableBuilder::new(ui)
            .striped(true)
            .resizable(false)
            .vscroll(true)
            .max_scroll_height(table_height)
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .column(Column::initial(84.0).at_least(74.0))
            .column(Column::remainder().at_least(150.0))
            .column(Column::initial(130.0).at_least(110.0))
            .column(Column::initial(160.0).at_least(120.0))
            .column(Column::initial(64.0).at_least(56.0))
            .header(24.0, |mut header| {
                header.col(|ui| {
                    ui.label(egui::RichText::new("Status").strong());
                });
                header.col(|ui| {
                    ui.label(egui::RichText::new("Name").strong());
                });
                header.col(|ui| {
                    ui.label(egui::RichText::new("Rank").strong());
                });
                header.col(|ui| {
                    ui.label(egui::RichText::new("Class").strong());
                });
                header.col(|ui| {
                    ui.label(egui::RichText::new("LVL").strong());
                });
            })
            .body(|body| {
                body.rows(22.0, clan.members.len(), |index, mut row| {
                    let member = &clan.members[index];
                    let is_online = member.channel_id.is_some();
                    let status_color = if is_online {
                        egui::Color32::from_rgb(95, 230, 116)
                    } else {
                        egui::Color32::from_rgb(142, 142, 142)
                    };

                    let class_name = game_data.string_database.get_job_name(member.job);
                    let class_name = if class_name.is_empty() {
                        format!("Job {}", member.job)
                    } else {
                        class_name.to_string()
                    };

                    row.col(|ui| {
                        ui.colored_label(status_color, if is_online { "Online" } else { "Offline" });
                    });
                    row.col(|ui| {
                        let is_selected = ui_state
                            .selected_member_name
                            .as_ref()
                            .map_or(false, |selected_name| selected_name == &member.name);
                        if ui.selectable_label(is_selected, &member.name).clicked() {
                            ui_state.selected_member_name = Some(member.name.clone());
                        }
                    });
                    row.col(|ui| {
                        ui.label(clan_position_name(game_data, member.position));
                    });
                    row.col(|ui| {
                        ui.label(class_name);
                    });
                    row.col(|ui| {
                        ui.label(format!("{}", member.level.level));
                    });
                });
            });
    }

    ui.separator();

    let max_members = game_data
        .ability_value_calculator
        .calculate_clan_max_members(clan.level.0);
    let can_manage_members = matches!(
        clan_membership.position,
        ClanMemberPosition::Master | ClanMemberPosition::DeputyMaster
    );

    let is_master = clan_membership.position == ClanMemberPosition::Master;
    let can_leave = !(is_master && clan.members.len() > 1);

    let (invite_target_name, invite_invalid_reason) =
        resolve_invite_target(selected_target, selected_target_query, player_entity);

    let mut can_expel_selected = false;
    let mut expel_selected_name = String::new();
    let expel_disabled_reason = if let Some(selected_member_name) =
        ui_state.selected_member_name.as_ref()
    {
        let is_self = player_name.map_or(false, |name| selected_member_name == name);
        if is_self {
            "You cannot expel yourself."
        } else if clan
            .find_member(selected_member_name)
            .map_or(false, |member| member.position == ClanMemberPosition::Master)
        {
            "You cannot expel the clan master."
        } else {
            can_expel_selected = true;
            expel_selected_name = selected_member_name.clone();
            ""
        }
    } else {
        "Select a member first."
    };

    let mut can_promote_selected = false;
    let mut promote_selected_name = String::new();
    let mut promote_target_position_label = String::new();
    let promote_disabled_reason = if let Some(selected_member_name) =
        ui_state.selected_member_name.as_ref()
    {
        let is_self = player_name.map_or(false, |name| selected_member_name == name);
        if is_self {
            "You cannot promote yourself."
        } else if let Some(selected_member) = clan.find_member(selected_member_name) {
            if !can_manage_members {
                "Only clan master and deputy master can promote members."
            } else {
                let actor_rank = position_to_rank(clan_membership.position);
                let target_rank = position_to_rank(selected_member.position);
                if target_rank >= actor_rank {
                    "You can only promote members below your rank."
                } else if target_rank.checked_add(1).map_or(true, |rank| rank >= actor_rank) {
                    "You cannot promote a member to your rank."
                } else if let Some(next_position) =
                    next_promoted_position(clan_membership.position, selected_member.position)
                {
                    can_promote_selected = true;
                    promote_selected_name = selected_member_name.clone();
                    promote_target_position_label = clan_position_name(game_data, next_position);
                    ""
                } else {
                    "Selected member cannot be promoted."
                }
            }
        } else {
            "Selected member is no longer in this clan."
        }
    } else {
        "Select a member first."
    };

    let mut can_demote_selected = false;
    let mut demote_selected_name = String::new();
    let mut demote_target_position_label = String::new();
    let demote_disabled_reason = if let Some(selected_member_name) =
        ui_state.selected_member_name.as_ref()
    {
        let is_self = player_name.map_or(false, |name| selected_member_name == name);
        if is_self {
            "You cannot demote yourself."
        } else if let Some(selected_member) = clan.find_member(selected_member_name) {
            if !can_manage_members {
                "Only clan master and deputy master can demote members."
            } else {
                let actor_rank = position_to_rank(clan_membership.position);
                let target_rank = position_to_rank(selected_member.position);
                if target_rank >= actor_rank {
                    "You can only demote members below your rank."
                } else if target_rank == 0 {
                    "Selected member is already at the lowest rank."
                } else if let Some(next_position) =
                    next_demoted_position(clan_membership.position, selected_member.position)
                {
                    can_demote_selected = true;
                    demote_selected_name = selected_member_name.clone();
                    demote_target_position_label = clan_position_name(game_data, next_position);
                    ""
                } else {
                    "Selected member cannot be demoted."
                }
            }
        } else {
            "Selected member is no longer in this clan."
        }
    } else {
        "Select a member first."
    };

    ui.horizontal(|ui| {
        ui.label(format!("Members: {} / {}", clan.members.len(), max_members));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if can_manage_members {
                let mut expel_response =
                    ui.add_enabled(can_expel_selected, egui::Button::new("Expel"));
                if !can_expel_selected {
                    expel_response = expel_response.on_hover_text(expel_disabled_reason);
                }
                if can_expel_selected && expel_response.clicked() {
                    ui_state.pending_action_confirm = Some(ClanActionConfirm::Expel {
                        name: expel_selected_name.clone(),
                    });
                }

                let mut demote_response =
                    ui.add_enabled(can_demote_selected, egui::Button::new("Demote"));
                if !can_demote_selected {
                    demote_response = demote_response.on_hover_text(demote_disabled_reason);
                }
                if can_demote_selected && demote_response.clicked() {
                    ui_state.pending_action_confirm = Some(ClanActionConfirm::Demote {
                        name: demote_selected_name.clone(),
                        next_position_label: demote_target_position_label.clone(),
                    });
                }

                let mut promote_response =
                    ui.add_enabled(can_promote_selected, egui::Button::new("Promote"));
                if !can_promote_selected {
                    promote_response = promote_response.on_hover_text(promote_disabled_reason);
                }
                if can_promote_selected && promote_response.clicked() {
                    ui_state.pending_action_confirm = Some(ClanActionConfirm::Promote {
                        name: promote_selected_name.clone(),
                        next_position_label: promote_target_position_label.clone(),
                    });
                }

                let mut invite_response = ui.add_enabled(
                    invite_target_name.is_some(),
                    egui::Button::new("Invite"),
                );
                if invite_target_name.is_none() {
                    invite_response = invite_response.on_hover_text(&invite_invalid_reason);
                }
                if let Some(invite_target_name) = invite_target_name.as_ref() {
                    if invite_response.clicked() {
                        if let Some(game_connection) = game_connection {
                            game_connection
                                .client_message_tx
                                .send(ClientMessage::ClanInvite {
                                    name: invite_target_name.clone(),
                                })
                                .ok();
                        }
                    }
                }
            }

            let mut leave_response = ui.add_enabled(can_leave, egui::Button::new("Leave"));
            if !can_leave {
                leave_response = leave_response
                    .on_hover_text("Clan master can only leave when they are the last member.");
            }
            if can_leave && leave_response.clicked() {
                ui_state.pending_action_confirm = Some(ClanActionConfirm::Leave);
            }
        });
    });
}

fn draw_clan_action_confirm_dialog(
    ctx: &egui::Context,
    ui_state: &mut UiStateClan,
    game_connection: Option<&GameConnection>,
) {
    let Some(pending_action) = ui_state.pending_action_confirm.clone() else {
        return;
    };

    let mut window_open = true;
    let mut cancel_clicked = false;
    let mut confirmed = false;

    let message = match &pending_action {
        ClanActionConfirm::Leave => "Leave clan?".to_string(),
        ClanActionConfirm::Expel { name } => format!("Expel {}?", name),
        ClanActionConfirm::Promote {
            name,
            next_position_label,
        } => format!("Promote {} to {}?", name, next_position_label),
        ClanActionConfirm::Demote {
            name,
            next_position_label,
        } => format!("Demote {} to {}?", name, next_position_label),
    };

    egui::Window::new("Confirm")
        .id(egui::Id::new("clan_action_confirm_dialog"))
        .collapsible(false)
        .resizable(false)
        .pivot(egui::Align2::CENTER_CENTER)
        .default_pos(ctx.screen_rect().center())
        .open(&mut window_open)
        .show(ctx, |ui| {
            ui.label(&message);
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if ui.button("Confirm").clicked() {
                    confirmed = true;
                }
                if ui.button("Cancel").clicked() {
                    cancel_clicked = true;
                }
            });
        });

    if confirmed {
        if let Some(game_connection) = game_connection {
            match pending_action {
                ClanActionConfirm::Leave => {
                    game_connection
                        .client_message_tx
                        .send(ClientMessage::ClanLeave)
                        .ok();
                }
                ClanActionConfirm::Expel { name } => {
                    game_connection
                        .client_message_tx
                        .send(ClientMessage::ClanKick { name })
                        .ok();
                }
                ClanActionConfirm::Promote { name, .. } => {
                    game_connection
                        .client_message_tx
                        .send(ClientMessage::ClanPromote { name })
                        .ok();
                }
                ClanActionConfirm::Demote { name, .. } => {
                    game_connection
                        .client_message_tx
                        .send(ClientMessage::ClanDemote { name })
                        .ok();
                }
            }
        }
        ui_state.pending_action_confirm = None;
    } else if !window_open || cancel_clicked {
        ui_state.pending_action_confirm = None;
    }
}

pub fn ui_clan_system(
    mut egui_context: EguiContexts,
    query_clan: Query<(&Clan, &ClanMembership), With<PlayerCharacter>>,
    query_player_entity: Query<Entity, With<PlayerCharacter>>,
    query_player_name: Query<&ClientEntityName, With<PlayerCharacter>>,
    query_selected_target: Query<(&ClientEntity, &ClientEntityName)>,
    mut ui_state: Local<UiStateClan>,
    mut ui_state_windows: ResMut<UiStateWindows>,
    mut clan_dialog_events: EventReader<ClanDialogEvent>,
    game_data: Res<GameData>,
    selected_target: Res<SelectedTarget>,
    game_connection: Option<Res<GameConnection>>,
) {
    let clan_result = query_clan.get_single();

    for event in clan_dialog_events.iter() {
        if matches!(event, ClanDialogEvent::Open) && clan_result.is_ok() {
            ui_state_windows.clan_open = true;
        }
    }

    let has_clan = clan_result.is_ok();
    if ui_state.had_clan_last_frame && !has_clan {
        ui_state.is_editing_slogan = false;
        ui_state.slogan_edit_buffer.clear();
        ui_state.selected_member_name = None;
        ui_state.pending_action_confirm = None;
    }

    let just_opened = ui_state_windows.clan_open && !ui_state.was_open;
    let min_window_size = egui::vec2(680.0, 420.0);
    let default_window_size = ui_state.last_window_size.unwrap_or(egui::vec2(820.0, 560.0));
    let screen_rect = egui_context.ctx_mut().screen_rect();
    let centered_pos = egui::pos2(
        screen_rect.center().x - default_window_size.x * 0.5,
        screen_rect.center().y - default_window_size.y * 0.5,
    );

    let mut window = egui::Window::new("Clan")
        .id(egui::Id::new("clan_window"))
        .open(&mut ui_state_windows.clan_open)
        .resizable(true)
        .default_size(default_window_size)
        .default_pos(centered_pos)
        .min_width(min_window_size.x)
        .min_height(min_window_size.y)
        .frame(egui::Frame::none()
            .fill(egui::Color32::from_rgba_unmultiplied(10, 10, 10, 235))
            .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(82, 82, 82)))
            .inner_margin(egui::Margin::same(8.0)));

    if just_opened && !ui_state.has_centered_once {
        window = window.current_pos(centered_pos);
    }

    let window_response = window.show(egui_context.ctx_mut(), |ui| {
            ui.horizontal(|ui| {
                if draw_tab_button(ui, "Clan Info", ui_state.active_tab == ClanTab::Info).clicked() {
                    ui_state.active_tab = ClanTab::Info;
                }
                if draw_tab_button(ui, "Members", ui_state.active_tab == ClanTab::Members).clicked()
                {
                    ui_state.active_tab = ClanTab::Members;
                }
            });

            ui.add_space(8.0);
            egui::Frame::none()
                .fill(egui::Color32::from_rgba_unmultiplied(0, 0, 0, 110))
                .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(62, 62, 62)))
                .inner_margin(egui::Margin::same(10.0))
                .show(ui, |ui| match clan_result {
                    Ok((clan, clan_membership)) => match ui_state.active_tab {
                        ClanTab::Info => draw_clan_info_tab(
                            ui,
                            clan,
                            clan_membership,
                            &game_data,
                            &mut ui_state,
                            game_connection.as_deref(),
                        ),
                        ClanTab::Members => draw_clan_members_tab(
                            ui,
                            clan,
                            clan_membership,
                            &game_data,
                            &mut ui_state,
                            game_connection.as_deref(),
                            &selected_target,
                            &query_selected_target,
                            query_player_entity.get_single().ok(),
                            query_player_name.get_single().ok().map(|name| name.name.as_str()),
                        ),
                    },
                    Err(_) => {
                        ui_state.is_editing_slogan = false;
                        ui_state.slogan_edit_buffer.clear();
                        ui_state.selected_member_name = None;
                        ui_state.pending_action_confirm = None;
                        ui.with_layout(
                            egui::Layout::top_down(egui::Align::Center),
                            |ui| {
                                ui.label(
                                    egui::RichText::new("You are not in a clan.")
                                        .size(16.0)
                                        .color(egui::Color32::from_rgb(202, 202, 202)),
                                );
                            },
                        );
                    }
                });

            // Keep a small free area near the resize handle so content widgets
            // (especially table/scroll regions) don't steal drag interactions.
            ui.add_space(8.0);
        });

    if ui_state_windows.clan_open {
        draw_clan_action_confirm_dialog(
            egui_context.ctx_mut(),
            &mut ui_state,
            game_connection.as_deref(),
        );
    } else {
        ui_state.pending_action_confirm = None;
    }

    if let Some(window_response) = window_response {
        if has_clan {
            let screen_size = screen_rect.size();
            let mut persisted_size = window_response.response.rect.size();
            persisted_size.x = persisted_size
                .x
                .clamp(min_window_size.x, screen_size.x.max(min_window_size.x));
            persisted_size.y = persisted_size
                .y
                .clamp(min_window_size.y, screen_size.y.max(min_window_size.y));

            ui_state.last_window_size = Some(persisted_size);
        }

        if just_opened && !ui_state.has_centered_once {
            ui_state.has_centered_once = true;
        }
    }

    ui_state.had_clan_last_frame = has_clan;
    ui_state.was_open = ui_state_windows.clan_open;
}
