use std::num::NonZeroU16;

use bevy::prelude::{EventReader, EventWriter, Local, Query, Res, ResMut, With};
use bevy_egui::{egui, EguiContexts};
use rose_game_common::{components::ClanMark, messages::client::ClientMessage};

use crate::{
    components::{ClanMembership, PlayerCharacter},
    events::{ClanDialogEvent, MessageBoxEvent},
    resources::{GameConnection, GameData},
    ui::UiStateWindows,
};

pub struct UiCreateClanState {
    pub clan_name: String,
    pub clan_slogan: String,
}

impl Default for UiCreateClanState {
    fn default() -> Self {
        Self {
            clan_name: String::new(),
            clan_slogan: String::new(),
        }
    }
}

impl UiCreateClanState {
    fn clear(&mut self) {
        self.clan_name.clear();
        self.clan_slogan.clear();
    }
}

pub fn ui_create_clan_system(
    mut ui_state: Local<UiCreateClanState>,
    mut ui_state_windows: ResMut<UiStateWindows>,
    mut egui_context: EguiContexts,
    mut clan_dialog_events: EventReader<ClanDialogEvent>,
    mut message_box_events: EventWriter<MessageBoxEvent>,
    query_player: Query<(), With<PlayerCharacter>>,
    query_player_clan: Query<&ClanMembership, With<PlayerCharacter>>,
    game_connection: Option<Res<GameConnection>>,
    game_data: Res<GameData>,
) {
    let ui_state = &mut *ui_state;
    let player_exists = query_player.get_single().is_ok();
    let player_has_clan = query_player_clan.get_single().is_ok();

    for event in clan_dialog_events.iter() {
        if matches!(event, ClanDialogEvent::Open) {
            if player_has_clan {
                ui_state_windows.create_clan_open = false;
                ui_state.clear();
            } else if player_exists {
                ui_state.clear();
                ui_state_windows.create_clan_open = true;
            }
        }
    }

    if !ui_state_windows.create_clan_open {
        return;
    }

    let mut create_clicked = false;
    let mut cancel_clicked = false;
    let mut window_open = ui_state_windows.create_clan_open;

    egui::Window::new("Create Clan")
        .id(egui::Id::new("create_clan_window"))
        .collapsible(false)
        .resizable(false)
        .pivot(egui::Align2::CENTER_CENTER)
        .default_pos(egui_context.ctx_mut().screen_rect().center())
        .open(&mut window_open)
        .show(egui_context.ctx_mut(), |ui| {
            ui.vertical_centered(|ui| {
                ui.heading("Create Clan");
            });
            ui.add_space(8.0);
            ui.label("Clan Name");
            ui.add_sized(
                [380.0, 26.0],
                egui::TextEdit::singleline(&mut ui_state.clan_name),
            );
            ui.add_space(8.0);
            ui.label("Slogan");
            ui.add_sized(
                [380.0, 92.0],
                egui::TextEdit::multiline(&mut ui_state.clan_slogan)
                    .desired_rows(4)
                    .desired_width(f32::INFINITY),
            );
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if ui.button("Create").clicked() {
                    create_clicked = true;
                }
                if ui.button("Cancel").clicked() {
                    cancel_clicked = true;
                }
            });
        });

    ui_state_windows.create_clan_open = window_open;

    if cancel_clicked || !ui_state_windows.create_clan_open {
        ui_state_windows.create_clan_open = false;
        ui_state.clear();
        return;
    }

    if !create_clicked {
        return;
    }

    let clan_name = ui_state.clan_name.trim();
    if clan_name.is_empty() {
        message_box_events.send(MessageBoxEvent::Show {
            message: game_data.client_strings.invalid_name.into(),
            modal: true,
            ok: None,
            cancel: None,
        });
        return;
    }

    let clan_slogan = ui_state.clan_slogan.trim();
    if clan_slogan.is_empty() {
        message_box_events.send(MessageBoxEvent::Show {
            message: game_data.client_strings.clan_create_error_slogan.into(),
            modal: true,
            ok: None,
            cancel: None,
        });
        return;
    }

    if let Some(game_connection) = game_connection {
        game_connection
            .client_message_tx
            .send(ClientMessage::ClanCreate {
                name: clan_name.to_string(),
                description: clan_slogan.to_string(),
                mark: ClanMark::Premade {
                    background: NonZeroU16::new(1).unwrap(),
                    foreground: NonZeroU16::new(1).unwrap(),
                },
            })
            .ok();
    }

    ui_state.clear();
    if ui_state_windows.create_clan_open {
        ui_state_windows.create_clan_open = false;
    }
}
