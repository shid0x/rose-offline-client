use std::{cell::Cell, collections::HashMap};

use bevy::prelude::{Assets, EventWriter, Local, Query, Res, ResMut, With, World};
use bevy_egui::{egui, EguiContexts};
use rose_game_common::{
    components::{CharacterInfo, CharacterUniqueId},
    messages::{client::ClientMessage, FriendStatus},
};

use crate::{
    components::PlayerCharacter,
    events::MessageBoxEvent,
    resources::{GameConnection, SocialState, UiResources},
    ui::{
        widgets::{DataBindings, Dialog, Widget},
        UiSoundEvent, UiStateWindows,
    },
};

const IID_COMMUNITY_CLOSE: i32 = 10;
const IID_COMMUNITY_TABBED_PANE: i32 = 20;
const IID_COMMUNITY_TAB_FRIEND: i32 = 21;
const IID_COMMUNITY_REMOVE: i32 = 24;
const IID_COMMUNITY_ADD: i32 = 29;
const IID_COMMUNITY_FRIEND_LIST: i32 = 26;
const IID_COMMUNITY_CHATROOM_TAB: i32 = 31;
const IID_COMMUNITY_CHATROOM_CREATE: i32 = 39;
const IID_COMMUNITY_MAILBOX_TAB: i32 = 41;

const IID_ADD_FRIEND_CLOSE: i32 = 10;
const IID_ADD_FRIEND_CONFIRM: i32 = 11;
const IID_ADD_FRIEND_EDITBOX: i32 = 20;

const IID_PRIVATE_CHAT_CLOSE: i32 = 10;
const IID_PRIVATE_CHAT_EDITBOX: i32 = 20;
const IID_PRIVATE_CHAT_LISTBOX: i32 = 30;

const COMMUNITY_NOT_IMPLEMENTED_MESSAGE: &str = "Mailbox and chatrooms are not implemented yet.";

struct PrivateChatWindowState {
    input_text: String,
    open: bool,
    focus_input: bool,
}

impl Default for PrivateChatWindowState {
    fn default() -> Self {
        Self {
            input_text: String::new(),
            open: true,
            focus_input: true,
        }
    }
}

pub struct UiCommunityState {
    current_tab: i32,
    selected_friend_index: i32,
    scroll_index: i32,
    add_friend_open: bool,
    add_friend_name: String,
    add_friend_focus_input: bool,
    private_chats: HashMap<CharacterUniqueId, PrivateChatWindowState>,
}

impl Default for UiCommunityState {
    fn default() -> Self {
        Self {
            current_tab: IID_COMMUNITY_TAB_FRIEND,
            selected_friend_index: 0,
            scroll_index: 0,
            add_friend_open: false,
            add_friend_name: String::new(),
            add_friend_focus_input: false,
            private_chats: HashMap::new(),
        }
    }
}

fn friend_status_text(status: FriendStatus) -> &'static str {
    match status {
        FriendStatus::Online => "Online",
        FriendStatus::Offline => "Offline",
        FriendStatus::Refused => "Refused",
        FriendStatus::Deleted => "Deleted",
    }
}

fn friend_status_color(status: FriendStatus) -> egui::Color32 {
    match status {
        FriendStatus::Online => egui::Color32::from_rgb(160, 255, 160),
        FriendStatus::Offline => egui::Color32::from_rgb(176, 176, 176),
        FriendStatus::Refused => egui::Color32::from_rgb(255, 192, 128),
        FriendStatus::Deleted => egui::Color32::from_rgb(255, 128, 128),
    }
}

fn draw_shadowed_text(
    ui: &egui::Ui,
    pos: egui::Pos2,
    text: impl AsRef<str>,
    font_id: egui::FontId,
    color: egui::Color32,
) {
    let text = text.as_ref();
    ui.painter().text(
        pos + egui::vec2(1.0, 1.0),
        egui::Align2::LEFT_TOP,
        text,
        font_id.clone(),
        egui::Color32::BLACK,
    );
    ui.painter()
        .text(pos, egui::Align2::LEFT_TOP, text, font_id, color);
}

fn ensure_private_chat_window(
    ui_state: &mut UiCommunityState,
    friend_id: CharacterUniqueId,
) -> &mut PrivateChatWindowState {
    let window = ui_state.private_chats.entry(friend_id).or_default();
    window.open = true;
    window.focus_input = true;
    window
}

fn send_friend_add_response(world: &mut World, requester_id: CharacterUniqueId, accept: bool) {
    if let Some(game_connection) = world.get_resource::<GameConnection>() {
        game_connection
            .client_message_tx
            .send(ClientMessage::FriendAddResponse {
                requester_id,
                accept,
            })
            .ok();
    }
}

pub fn ui_community_system(
    mut egui_context: EguiContexts,
    mut ui_state: Local<UiCommunityState>,
    mut ui_state_windows: ResMut<UiStateWindows>,
    mut ui_sound_events: EventWriter<UiSoundEvent>,
    mut message_box_events: EventWriter<MessageBoxEvent>,
    game_connection: Option<Res<GameConnection>>,
    query_player: Query<&CharacterInfo, With<PlayerCharacter>>,
    mut social_state: ResMut<SocialState>,
    ui_resources: Res<UiResources>,
    dialog_assets: Res<Assets<Dialog>>,
) {
    let Some(community_dialog) = dialog_assets.get(&ui_resources.dialog_community) else {
        return;
    };
    let Some(add_friend_dialog) = dialog_assets.get(&ui_resources.dialog_add_friend) else {
        return;
    };
    let Some(private_chat_dialog) = dialog_assets.get(&ui_resources.dialog_private_chat) else {
        return;
    };

    let player_name = query_player
        .get_single()
        .ok()
        .map(|player| player.name.clone())
        .unwrap_or_else(|| "You".to_string());

    for friend_id in social_state.take_open_chat_requests() {
        ensure_private_chat_window(&mut ui_state, friend_id);
    }

    let pending_requests = social_state.pending_requests.drain(..).collect::<Vec<_>>();
    for pending_request in pending_requests {
        let requester_id = pending_request.requester_id;
        let requester_name = pending_request.name;
        message_box_events.send(MessageBoxEvent::Show {
            message: format!("{} wants to add you as a friend.", requester_name),
            modal: true,
            ok: Some(Box::new(move |commands| {
                commands.add(move |world: &mut World| {
                    send_friend_add_response(world, requester_id, true);
                });
            })),
            cancel: Some(Box::new(move |commands| {
                commands.add(move |world: &mut World| {
                    send_friend_add_response(world, requester_id, false);
                });
            })),
        });
    }

    let friend_count = social_state.friends.len() as i32;
    let listbox_extent = if let Some(Widget::ZListbox(listbox)) =
        community_dialog.get_widget(IID_COMMUNITY_FRIEND_LIST)
    {
        listbox.extent
    } else {
        14
    };
    let max_scroll_index = (friend_count - listbox_extent).max(0);
    ui_state.scroll_index = ui_state.scroll_index.clamp(0, max_scroll_index);
    ui_state.selected_friend_index = if friend_count == 0 {
        0
    } else {
        ui_state.selected_friend_index.clamp(0, friend_count - 1)
    };

    let selected_friend = social_state
        .friends
        .get(ui_state.selected_friend_index as usize)
        .cloned();
    let open_friend_id = Cell::new(None);
    let mut response_close_button = None;
    let mut response_add_button = None;
    let mut response_remove_button = None;

    if ui_state_windows.community_open {
        egui::Window::new("Community")
            .frame(egui::Frame::none())
            .open(&mut ui_state_windows.community_open)
            .title_bar(false)
            .resizable(false)
            .default_width(community_dialog.width)
            .default_height(community_dialog.height)
            .show(egui_context.ctx_mut(), |ui| {
                let UiCommunityState {
                    current_tab,
                    scroll_index,
                    selected_friend_index,
                    ..
                } = &mut *ui_state;
                community_dialog.draw(
                    ui,
                    DataBindings {
                        sound_events: Some(&mut ui_sound_events),
                        enabled: &mut [(IID_COMMUNITY_REMOVE, selected_friend.is_some())],
                        tabs: &mut [(IID_COMMUNITY_TABBED_PANE, current_tab)],
                        scroll: &mut [(
                            IID_COMMUNITY_FRIEND_LIST,
                            (scroll_index, 0..friend_count, listbox_extent),
                        )],
                        zlist: &mut [(
                            IID_COMMUNITY_FRIEND_LIST,
                            (selected_friend_index, &|ui, index, selected| {
                                let row_height = 20.0;
                                let row_width = 180.0;
                                let (rect, response) = ui.allocate_exact_size(
                                    egui::vec2(row_width, row_height),
                                    egui::Sense::click(),
                                );

                                if selected {
                                    ui.painter().rect_filled(
                                        rect,
                                        0.0,
                                        egui::Color32::from_rgba_unmultiplied(136, 98, 40, 128),
                                    );
                                }

                                if let Some(friend) = social_state.friends.get(index as usize) {
                                    let marker_sprite = match friend.status {
                                        FriendStatus::Online => {
                                            ui_resources.get_sprite(0, "CLAN01_MARK_ONLINE")
                                        }
                                        _ => ui_resources.get_sprite(0, "CLAN01_MARK_OFFLINE"),
                                    };

                                    if let Some(marker_sprite) = marker_sprite {
                                        marker_sprite.draw(ui, rect.min + egui::vec2(2.0, 3.0));
                                    } else {
                                        ui.painter().circle_filled(
                                            rect.min + egui::vec2(8.0, 9.0),
                                            4.0,
                                            friend_status_color(friend.status),
                                        );
                                    }

                                    draw_shadowed_text(
                                        ui,
                                        rect.min + egui::vec2(20.0, 3.0),
                                        format!(
                                            "{}({})",
                                            friend.name,
                                            friend_status_text(friend.status)
                                        ),
                                        egui::FontId::proportional(12.0),
                                        if selected {
                                            egui::Color32::from_rgb(255, 220, 96)
                                        } else if matches!(friend.status, FriendStatus::Online) {
                                            egui::Color32::BLACK
                                        } else {
                                            friend_status_color(friend.status)
                                        },
                                    );

                                    if response.double_clicked()
                                        && matches!(friend.status, FriendStatus::Online)
                                    {
                                        open_friend_id.set(Some(friend.character_id));
                                    }
                                }

                                response
                            }),
                        )],
                        response: &mut [
                            (IID_COMMUNITY_CLOSE, &mut response_close_button),
                            (IID_COMMUNITY_ADD, &mut response_add_button),
                            (IID_COMMUNITY_REMOVE, &mut response_remove_button),
                        ],
                        ..Default::default()
                    },
                    |_, _| {},
                );
            });
    }

    if response_close_button.map_or(false, |response| response.clicked()) {
        ui_state_windows.community_open = false;
    }

    if ui_state.current_tab == IID_COMMUNITY_CHATROOM_TAB
        || ui_state.current_tab == IID_COMMUNITY_MAILBOX_TAB
    {
        ui_state.current_tab = IID_COMMUNITY_TAB_FRIEND;
        message_box_events.send(MessageBoxEvent::Show {
            message: COMMUNITY_NOT_IMPLEMENTED_MESSAGE.to_string(),
            modal: false,
            ok: None,
            cancel: None,
        });
    }

    if response_add_button.map_or(false, |response| response.clicked()) {
        ui_state.add_friend_open = true;
        ui_state.add_friend_focus_input = true;
        ui_state.add_friend_name.clear();
    }

    if response_remove_button.map_or(false, |response| response.clicked()) {
        if let Some(friend) = selected_friend {
            let friend_id = friend.character_id;
            let friend_name = friend.name;
            message_box_events.send(MessageBoxEvent::Show {
                message: format!("Remove {} from your friend list?", friend_name),
                modal: true,
                ok: Some(Box::new(move |commands| {
                    commands.add(move |world: &mut World| {
                        if let Some(mut social_state) = world.get_resource_mut::<SocialState>() {
                            social_state.remove_friend(friend_id);
                        }

                        if let Some(game_connection) = world.get_resource::<GameConnection>() {
                            game_connection
                                .client_message_tx
                                .send(ClientMessage::FriendRemove { friend_id })
                                .ok();
                        }
                    });
                })),
                cancel: None,
            });
        }
    }

    if let Some(friend_id) = open_friend_id.get() {
        ensure_private_chat_window(&mut ui_state, friend_id);
    }

    if ui_state.add_friend_open {
        let mut add_friend_open = ui_state.add_friend_open;
        let mut response_close_button = None;
        let mut response_confirm_button = None;
        let mut response_editbox = None;
        let mut submit = false;

        egui::Window::new("Add Friend")
            .id(egui::Id::new("community_add_friend"))
            .frame(egui::Frame::none())
            .open(&mut add_friend_open)
            .title_bar(false)
            .resizable(false)
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(egui_context.ctx_mut().screen_rect().center())
            .default_width(add_friend_dialog.width)
            .default_height(add_friend_dialog.height)
            .show(egui_context.ctx_mut(), |ui| {
                let add_friend_name = &mut ui_state.add_friend_name;
                add_friend_dialog.draw(
                    ui,
                    DataBindings {
                        sound_events: Some(&mut ui_sound_events),
                        text: &mut [(IID_ADD_FRIEND_EDITBOX, add_friend_name)],
                        response: &mut [
                            (IID_ADD_FRIEND_CLOSE, &mut response_close_button),
                            (IID_ADD_FRIEND_CONFIRM, &mut response_confirm_button),
                            (IID_ADD_FRIEND_EDITBOX, &mut response_editbox),
                        ],
                        ..Default::default()
                    },
                    |_, _| {},
                );
            });
        ui_state.add_friend_open = add_friend_open;

        if let Some(response_editbox) = response_editbox.as_ref() {
            if ui_state.add_friend_focus_input {
                response_editbox.request_focus();
                ui_state.add_friend_focus_input = false;
            }

            if response_editbox
                .ctx
                .input(|input| input.key_pressed(egui::Key::Enter))
            {
                if response_editbox.lost_focus() {
                    submit = true;
                } else {
                    response_editbox.request_focus();
                }
            }
        }

        if response_close_button.map_or(false, |response| response.clicked()) {
            ui_state.add_friend_open = false;
        }

        if response_confirm_button.map_or(false, |response| response.clicked()) {
            submit = true;
        }

        if submit {
            let requested_name = ui_state.add_friend_name.trim().to_string();
            let is_duplicate = social_state
                .friends
                .iter()
                .any(|friend| friend.name.eq_ignore_ascii_case(&requested_name));

            if !requested_name.is_empty()
                && !requested_name.eq_ignore_ascii_case(&player_name)
                && !is_duplicate
            {
                if let Some(game_connection) = game_connection.as_ref() {
                    game_connection
                        .client_message_tx
                        .send(ClientMessage::FriendAdd {
                            name: requested_name,
                        })
                        .ok();
                }

                ui_state.add_friend_name.clear();
                ui_state.add_friend_open = false;
            }
        }
    }

    let mut chat_window_ids = ui_state
        .private_chats
        .keys()
        .copied()
        .collect::<Vec<CharacterUniqueId>>();
    chat_window_ids.sort_unstable();

    for friend_id in chat_window_ids {
        let Some(chat_window) = ui_state.private_chats.get_mut(&friend_id) else {
            continue;
        };

        let friend = social_state.get_friend(friend_id).cloned();
        let friend_name = friend
            .as_ref()
            .map(|friend| friend.name.clone())
            .unwrap_or_else(|| format!("Friend {}", friend_id));
        let friend_status = friend
            .as_ref()
            .map(|friend| friend.status)
            .unwrap_or(FriendStatus::Offline);
        let can_send = matches!(friend_status, FriendStatus::Online);
        let messages = social_state
            .chat_histories
            .get(&friend_id)
            .cloned()
            .unwrap_or_default();

        let mut response_close_button = None;
        let mut response_editbox = None;
        let mut send_message = false;
        let mut open = chat_window.open;

        egui::Window::new(format!("Private Chat {}", friend_id))
            .id(egui::Id::new(format!("private_chat_{}", friend_id)))
            .frame(egui::Frame::none())
            .open(&mut open)
            .title_bar(false)
            .resizable(false)
            .default_width(private_chat_dialog.width)
            .default_height(private_chat_dialog.height)
            .show(egui_context.ctx_mut(), |ui| {
                private_chat_dialog.draw(
                    ui,
                    DataBindings {
                        sound_events: Some(&mut ui_sound_events),
                        enabled: &mut [(IID_PRIVATE_CHAT_EDITBOX, can_send)],
                        text: &mut [(IID_PRIVATE_CHAT_EDITBOX, &mut chat_window.input_text)],
                        response: &mut [
                            (IID_PRIVATE_CHAT_CLOSE, &mut response_close_button),
                            (IID_PRIVATE_CHAT_EDITBOX, &mut response_editbox),
                        ],
                        ..Default::default()
                    },
                    |ui, _| {
                        draw_shadowed_text(
                            ui,
                            ui.min_rect().min + egui::vec2(30.0, 8.0),
                            format!("To:{}({})", friend_name, friend_status_text(friend_status)),
                            egui::FontId::proportional(14.0),
                            egui::Color32::from_rgb(232, 136, 28),
                        );

                        if let Some(Widget::Listbox(listbox)) =
                            private_chat_dialog.get_widget(IID_PRIVATE_CHAT_LISTBOX)
                        {
                            let rect = listbox
                                .widget_rect(ui.min_rect().min)
                                .shrink2(egui::vec2(2.0, 2.0));
                            ui.allocate_ui_at_rect(rect, |ui| {
                                egui::ScrollArea::vertical()
                                    .auto_shrink([false; 2])
                                    .stick_to_bottom(true)
                                    .show(ui, |ui| {
                                        for message in &messages {
                                            ui.horizontal_wrapped(|ui| {
                                                ui.colored_label(
                                                    egui::Color32::BLACK,
                                                    format!(
                                                        "{}> {}",
                                                        message.from_name, message.text
                                                    ),
                                                );
                                            });
                                        }
                                    });
                            });
                        }
                    },
                );
            });

        chat_window.open = open;

        if let Some(response_editbox) = response_editbox.as_ref() {
            if chat_window.focus_input {
                response_editbox.request_focus();
                chat_window.focus_input = false;
            }

            if response_editbox
                .ctx
                .input(|input| input.key_pressed(egui::Key::Enter))
            {
                if response_editbox.lost_focus() {
                    send_message = true;
                } else {
                    response_editbox.request_focus();
                }
            }
        }

        if response_close_button.map_or(false, |response| response.clicked()) {
            chat_window.open = false;
        }

        if send_message && can_send {
            let text = chat_window.input_text.trim().to_string();
            if !text.is_empty() {
                if let Some(game_connection) = game_connection.as_ref() {
                    game_connection
                        .client_message_tx
                        .send(ClientMessage::FriendChat {
                            friend_id,
                            text: text.clone(),
                        })
                        .ok();
                }
                social_state.append_chat_message(friend_id, player_name.clone(), text, true);
                chat_window.input_text.clear();
            }
        }
    }

    ui_state.private_chats.retain(|_, window| window.open);

    let _ = IID_COMMUNITY_CHATROOM_CREATE;
}
