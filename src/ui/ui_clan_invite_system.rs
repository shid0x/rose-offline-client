use bevy::prelude::{Res, ResMut};
use bevy_egui::{egui, EguiContexts};
use rose_game_common::messages::client::ClientMessage;

use crate::resources::{GameConnection, PendingClanInvites};

pub fn ui_clan_invite_system(
    mut egui_context: EguiContexts,
    mut pending_clan_invites: ResMut<PendingClanInvites>,
    game_connection: Option<Res<GameConnection>>,
) {
    let mut i = 0;
    while i < pending_clan_invites.invites.len() {
        let mut accepted = false;
        let mut rejected = false;

        let inviter_name = pending_clan_invites.invites[i].inviter_name.clone();
        let clan_name = pending_clan_invites.invites[i].clan_name.clone();
        let clan_level = pending_clan_invites.invites[i].clan_level.0;

        let mut window_open = true;
        egui::Window::new("Clan Invite")
            .id(egui::Id::new(format!("clan_invite_{}", &inviter_name)))
            .collapsible(false)
            .resizable(false)
            .default_pos(egui_context.ctx_mut().screen_rect().center())
            .pivot(egui::Align2::CENTER_CENTER)
            .open(&mut window_open)
            .show(egui_context.ctx_mut(), |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(8.0);
                    ui.label(
                        egui::RichText::new(format!(
                            "{} has invited you to join clan \"{}\" (Lv.{})",
                            &inviter_name, &clan_name, clan_level,
                        ))
                        .size(16.0),
                    );
                    ui.add_space(16.0);
                });
                ui.vertical_centered(|ui| {
                    ui.horizontal(|ui| {
                        let total_width = ui.available_width();
                        let button_width = 80.0;
                        let spacing = 16.0;
                        let offset = (total_width - button_width * 2.0 - spacing) / 2.0;
                        ui.add_space(offset);
                        if ui.add_sized([button_width, 28.0], egui::Button::new(egui::RichText::new("Accept").size(15.0))).clicked() {
                            accepted = true;
                        }
                        ui.add_space(spacing);
                        if ui.add_sized([button_width, 28.0], egui::Button::new(egui::RichText::new("Reject").size(15.0))).clicked() {
                            rejected = true;
                        }
                    });
                    ui.add_space(8.0);
                });
            });

        if !window_open {
            rejected = true;
        }

        if accepted {
            if let Some(game_connection) = &game_connection {
                game_connection
                    .client_message_tx
                    .send(ClientMessage::ClanAcceptInvite { inviter_name: inviter_name.clone() })
                    .ok();
            }
            pending_clan_invites.invites.remove(i);
            continue;
        } else if rejected {
            if let Some(game_connection) = &game_connection {
                game_connection
                    .client_message_tx
                    .send(ClientMessage::ClanRejectInvite { inviter_name: inviter_name.clone() })
                    .ok();
            }
            pending_clan_invites.invites.remove(i);
            continue;
        }

        i += 1;
    }
}
