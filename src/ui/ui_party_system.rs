use bevy::{
    ecs::query::WorldQuery,
    prelude::{Assets, Entity, EventReader, EventWriter, Local, Query, Res, ResMut, With},
};
use bevy_egui::{egui, EguiContexts};

use rose_game_common::{
    components::{AbilityValues, CharacterInfo, CharacterUniqueId, HealthPoints, Level},
    messages::{
        client::ClientMessage, server::PartyMemberInfo, server::PartyMemberInfoOnline,
        ClientEntityId, PartyRejectInviteReason,
    },
};

use crate::{
    components::{ClientEntity, ClientEntityName, PartyInfo, PartyOwner, PlayerCharacter},
    events::PartyEvent,
    resources::{ClientEntityList, GameConnection, SelectedTarget, UiResources},
    ui::{
        widgets::{Dialog, Gauge},
        UiSoundEvent,
    },
};

use super::{
    widgets::{DrawText, DrawWidget, LoadWidget},
    DataBindings, UiStateWindows,
};

const IID_BTN_ENTRUST: i32 = 11;
const IID_BTN_BAN: i32 = 12;
const IID_BTN_LEAVE: i32 = 13;
const IID_BTN_OPTION: i32 = 14;
const IID_PARTY_XP_GAUGE: i32 = 1001;
const IID_PARTY_MEMBER_HP_GAUGE: i32 = 1002;
const PARTY_WINDOW_RIGHT_MARGIN: f32 = 24.0;
const PARTY_WINDOW_TOP_OFFSET: f32 = 300.0;
const PARTY_INVITE_WINDOW_WIDTH: f32 = 360.0;
const PARTY_INVITE_BUTTON_WIDTH: f32 = 120.0;
const PARTY_INVITE_BUTTON_HEIGHT: f32 = 36.0;

#[derive(Clone, Copy, Debug)]
enum PartyDisplayRow<'a> {
    Player,
    Member(&'a PartyMemberInfo),
}

fn compute_hp_percent(hp: i32, max_hp: i32) -> f32 {
    if max_hp > 0 {
        (hp as f32 / max_hp as f32).clamp(0.0, 1.0)
    } else {
        0.0
    }
}

fn compute_party_member_hp_percent(
    member_info: &PartyMemberInfoOnline,
    fallback: Option<(i32, i32)>,
) -> f32 {
    let (hp, max_hp) = if member_info.max_health > 0 {
        (member_info.health_points.hp, member_info.max_health)
    } else {
        fallback.unwrap_or((member_info.health_points.hp, member_info.max_health))
    };

    compute_hp_percent(hp, max_hp)
}

fn build_party_display_rows<'a>(
    owner: &PartyOwner,
    members: &'a [PartyMemberInfo],
) -> Vec<PartyDisplayRow<'a>> {
    let insert_index = match owner {
        PartyOwner::Player => 0,
        PartyOwner::Character(owner_character_id) => members
            .iter()
            .position(|member| member.get_character_id() == *owner_character_id)
            .map(|index| index + 1)
            .unwrap_or(0),
        PartyOwner::Unknown => 0,
    };

    let mut rows = members
        .iter()
        .map(PartyDisplayRow::Member)
        .collect::<Vec<_>>();
    rows.insert(insert_index.min(rows.len()), PartyDisplayRow::Player);
    rows
}

#[derive(WorldQuery)]
pub struct PlayerQuery<'w> {
    _player_character: With<PlayerCharacter>,
    entity: Entity,
    ability_values: &'w AbilityValues,
    character_info: &'w CharacterInfo,
    health_points: &'w HealthPoints,
    level: &'w Level,
    party_info: Option<&'w PartyInfo>,
}

#[derive(WorldQuery)]
pub struct PartyMemberQuery<'w> {
    character_info: &'w CharacterInfo,
    ability_values: &'w AbilityValues,
    health_points: &'w HealthPoints,
    level: &'w Level,
}

pub struct PendingPartyInvite {
    is_create: bool,
    client_entity_id: ClientEntityId,
    name: String,
}

pub struct UiStatePartySystem {
    pending_invites: Vec<PendingPartyInvite>,
    party_xp_gauge: Gauge,
    party_member_health_gauge: Gauge,
    selected_party_member_id: Option<CharacterUniqueId>,
}

impl Default for UiStatePartySystem {
    fn default() -> Self {
        Self {
            pending_invites: Default::default(),
            party_xp_gauge: Gauge {
                id: IID_PARTY_XP_GAUGE,
                x: 96.0,
                y: 34.0,
                width: 111.0,
                height: 9.0,
                module_id: 0,
                foreground_sprite_name: "UI18_GUAGE_PARTYLEVEL".into(),
                background_sprite_name: "UI18_GUAGE_PARTYLEVEL_BASE".into(),
                ..Default::default()
            },
            party_member_health_gauge: Gauge {
                id: IID_PARTY_MEMBER_HP_GAUGE,
                width: 119.0,
                height: 9.0,
                module_id: 0,
                foreground_sprite_name: "UI18_GUAGE_HP".into(),
                background_sprite_name: "UI18_GUAGE_HP_BASE".into(),
                ..Default::default()
            },
            selected_party_member_id: None,
        }
    }
}

pub fn ui_party_system(
    mut ui_state: Local<UiStatePartySystem>,
    mut ui_state_windows: ResMut<UiStateWindows>,
    mut ui_sound_events: EventWriter<UiSoundEvent>,
    mut egui_context: EguiContexts,
    query_player: Query<PlayerQuery>,
    query_party_member: Query<PartyMemberQuery>,
    query_invite: Query<(&ClientEntity, &ClientEntityName)>,
    mut party_events: EventReader<PartyEvent>,
    game_connection: Option<Res<GameConnection>>,
    client_entity_list: Res<ClientEntityList>,
    ui_resources: Res<UiResources>,
    dialog_assets: Res<Assets<Dialog>>,
    mut selected_target: ResMut<SelectedTarget>,
) {
    let player = if let Ok(player) = query_player.get_single() {
        player
    } else {
        return;
    };

    // Add any new incoming invites
    for event in party_events.iter() {
        match *event {
            PartyEvent::InvitedCreate(entity) => {
                if let Ok((client_entity, client_entity_name)) = query_invite.get(entity) {
                    ui_state.pending_invites.push(PendingPartyInvite {
                        is_create: true,
                        client_entity_id: client_entity.id,
                        name: client_entity_name.to_string(),
                    });
                }
            }
            PartyEvent::InvitedJoin(entity) => {
                if let Ok((client_entity, client_entity_name)) = query_invite.get(entity) {
                    ui_state.pending_invites.push(PendingPartyInvite {
                        is_create: false,
                        client_entity_id: client_entity.id,
                        name: client_entity_name.to_string(),
                    });
                }
            }
        }
    }

    let mut i = 0;
    while i != ui_state.pending_invites.len() {
        let mut window_open = true;
        let mut accepted = false;
        let mut rejected = false;
        let pending_invite = &ui_state.pending_invites[i];

        if player.party_info.is_none() {
            let invite_text = format!(
                "{} has invited you to {} a party.",
                &pending_invite.name,
                if pending_invite.is_create {
                    "create"
                } else {
                    "join"
                }
            );

            egui::Window::new("Party Invite")
                .id(egui::Id::new(format!(
                    "party_invite_{}",
                    &pending_invite.name
                )))
                .collapsible(false)
                .resizable(false)
                .default_width(PARTY_INVITE_WINDOW_WIDTH)
                .pivot(egui::Align2::CENTER_CENTER)
                .default_pos(egui_context.ctx_mut().screen_rect().center())
                .open(&mut window_open)
                .show(egui_context.ctx_mut(), |ui| {
                    ui.set_min_width(PARTY_INVITE_WINDOW_WIDTH);
                    ui.vertical_centered(|ui| {
                        ui.add_space(10.0);
                        ui.allocate_ui_with_layout(
                            egui::vec2(PARTY_INVITE_WINDOW_WIDTH - 40.0, 0.0),
                            egui::Layout::top_down(egui::Align::Center),
                            |ui| {
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(invite_text.clone()).size(18.0),
                                    )
                                    .wrap(true),
                                );
                            },
                        );
                        ui.add_space(18.0);
                        ui.horizontal(|ui| {
                            let total_width = ui.available_width();
                            let button_spacing = 20.0;
                            let button_row_width = PARTY_INVITE_BUTTON_WIDTH * 2.0 + button_spacing;
                            ui.add_space((total_width - button_row_width).max(0.0) * 0.5);

                            if ui
                                .add_sized(
                                    [PARTY_INVITE_BUTTON_WIDTH, PARTY_INVITE_BUTTON_HEIGHT],
                                    egui::Button::new(egui::RichText::new("Accept").size(17.0)),
                                )
                                .clicked()
                            {
                                accepted = true;
                            }

                            ui.add_space(button_spacing);

                            if ui
                                .add_sized(
                                    [PARTY_INVITE_BUTTON_WIDTH, PARTY_INVITE_BUTTON_HEIGHT],
                                    egui::Button::new(egui::RichText::new("Reject").size(17.0)),
                                )
                                .clicked()
                            {
                                rejected = true;
                            }
                        });
                        ui.add_space(10.0);
                    });
                });
        } else {
            rejected = true;
        }

        if !window_open {
            rejected = true;
        }

        if accepted {
            if let Some(game_connection) = &game_connection {
                if pending_invite.is_create {
                    game_connection
                        .client_message_tx
                        .send(ClientMessage::PartyAcceptCreateInvite {
                            owner_entity_id: pending_invite.client_entity_id,
                        })
                        .ok();
                } else {
                    game_connection
                        .client_message_tx
                        .send(ClientMessage::PartyAcceptJoinInvite {
                            owner_entity_id: pending_invite.client_entity_id,
                        })
                        .ok();
                }
            }

            ui_state.pending_invites.remove(i);
            continue;
        } else if rejected {
            if let Some(game_connection) = &game_connection {
                game_connection
                    .client_message_tx
                    .send(ClientMessage::PartyRejectInvite {
                        reason: PartyRejectInviteReason::Reject,
                        owner_entity_id: pending_invite.client_entity_id,
                    })
                    .ok();
            }

            ui_state.pending_invites.remove(i);
            continue;
        }

        i += 1;
    }

    let dialog = if let Some(dialog) = dialog_assets.get(&ui_resources.dialog_party) {
        if ui_state.party_xp_gauge.foreground_sprite.is_none() {
            ui_state.party_xp_gauge.load_widget(&ui_resources);
        }

        if ui_state
            .party_member_health_gauge
            .foreground_sprite
            .is_none()
        {
            ui_state
                .party_member_health_gauge
                .load_widget(&ui_resources);
        }

        dialog
    } else {
        return;
    };

    let mut response_entrust_button = None;
    let mut response_kick_button = None;
    let mut response_leave_button = None;
    let mut response_option_button = None;
    let screen_size = egui_context
        .ctx_mut()
        .input(|input| input.screen_rect().size());

    ui_state_windows.party_open = player.party_info.is_some();

    if let Some(party_info) = player.party_info {
        let player_is_owner = matches!(party_info.owner, PartyOwner::Player);
        let party_display_rows = build_party_display_rows(&party_info.owner, &party_info.members);

        // Compute party XP gauge values
        let party_level = party_info.level;
        let party_xp = party_info.experience;
        let need_xp = (party_level + 7) * (party_level + 10) + 40;
        let xp_pct = if need_xp > 0 {
            (party_xp as f32 / need_xp as f32).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let xp_label = format!("{:.0}%", xp_pct * 100.0);

        egui::Window::new("Party2")
            .default_pos(egui::pos2(
                (screen_size.x - dialog.width - PARTY_WINDOW_RIGHT_MARGIN).max(0.0),
                PARTY_WINDOW_TOP_OFFSET,
            ))
            .frame(egui::Frame::none())
            .title_bar(false)
            .resizable(false)
            .default_width(dialog.width)
            .default_height(dialog.height)
            .show(egui_context.ctx_mut(), |ui| {
                dialog.draw(
                    ui,
                    DataBindings {
                        sound_events: Some(&mut ui_sound_events),
                        gauge: &mut [(IID_PARTY_XP_GAUGE, &xp_pct, &xp_label)],
                        response: &mut [
                            (IID_BTN_ENTRUST, &mut response_entrust_button),
                            (IID_BTN_BAN, &mut response_kick_button),
                            (IID_BTN_LEAVE, &mut response_leave_button),
                            (IID_BTN_OPTION, &mut response_option_button),
                        ],
                        visible: &mut [
                            (IID_BTN_BAN, player_is_owner),
                            (IID_BTN_ENTRUST, player_is_owner),
                        ],
                        ..Default::default()
                    },
                    |ui, bindings| {
                        ui.add_label_at(
                            egui::pos2(35.0, 7.0),
                            egui::RichText::new("Party").color(egui::Color32::BLACK),
                        );

                        ui.add_label_at(
                            egui::pos2(17.0, 34.0),
                            format!("Party Level: {}", party_level),
                        );

                        ui_state.party_xp_gauge.draw_widget(ui, bindings);

                        ui.vertical(|ui| {
                            for row in party_display_rows.iter() {
                                let (rect, response) = ui.allocate_exact_size(
                                    egui::vec2(220.0, 45.0),
                                    egui::Sense::click(),
                                );
                                let (character_id, selected_client_entity) = {
                                    let ui = &mut ui.child_ui(rect, egui::Layout::default());
                                    let (character_id, online, name, selected_client_entity) =
                                        match row {
                                            PartyDisplayRow::Player => {
                                                let hp_percent = compute_hp_percent(
                                                    player.health_points.hp,
                                                    player.ability_values.get_max_health(),
                                                );

                                                ui_state.party_member_health_gauge.x = 220.0
                                                    - ui_state.party_member_health_gauge.width;
                                                ui_state.party_member_health_gauge.y = 25.0;
                                                ui_state.party_member_health_gauge.draw_widget(
                                                    ui,
                                                    &mut DataBindings {
                                                        gauge: &mut [(
                                                            IID_PARTY_MEMBER_HP_GAUGE,
                                                            &hp_percent,
                                                            &format!("{:.2}%", 100.0 * hp_percent),
                                                        )],
                                                        ..Default::default()
                                                    },
                                                );

                                                (
                                                    player.character_info.unique_id,
                                                    true,
                                                    player.character_info.name.as_str(),
                                                    Some(player.entity),
                                                )
                                            }
                                            PartyDisplayRow::Member(member) => match member {
                                                PartyMemberInfo::Online(member_info) => {
                                                    let fallback_hp = client_entity_list
                                                        .get(member_info.entity_id)
                                                        .and_then(|entity| {
                                                            query_party_member.get(entity).ok()
                                                        })
                                                        .map(|party_member| {
                                                            (
                                                                party_member.health_points.hp,
                                                                party_member
                                                                    .ability_values
                                                                    .get_max_health(),
                                                            )
                                                        });
                                                    let hp_percent =
                                                        compute_party_member_hp_percent(
                                                            member_info,
                                                            fallback_hp,
                                                        );

                                                    ui_state.party_member_health_gauge.x = 220.0
                                                        - ui_state.party_member_health_gauge.width;
                                                    ui_state.party_member_health_gauge.y = 25.0;
                                                    ui_state.party_member_health_gauge.draw_widget(
                                                        ui,
                                                        &mut DataBindings {
                                                            gauge: &mut [(
                                                                IID_PARTY_MEMBER_HP_GAUGE,
                                                                &hp_percent,
                                                                &format!(
                                                                    "{:.2}%",
                                                                    100.0 * hp_percent
                                                                ),
                                                            )],
                                                            ..Default::default()
                                                        },
                                                    );

                                                    (
                                                        member_info.character_id,
                                                        true,
                                                        member_info.name.as_str(),
                                                        client_entity_list
                                                            .get(member_info.entity_id),
                                                    )
                                                }
                                                PartyMemberInfo::Offline(member_info) => (
                                                    member_info.character_id,
                                                    false,
                                                    member_info.name.as_str(),
                                                    None,
                                                ),
                                            },
                                        };
                                    let selected =
                                        ui_state.selected_party_member_id == Some(character_id);

                                    ui.add_label_at(
                                        egui::pos2(4.0, 26.0),
                                        egui::RichText::new(name).color(egui::Color32::BLACK),
                                    );
                                    ui.add_label_at(
                                        egui::pos2(3.0, 25.0),
                                        egui::RichText::new(name).color(if selected {
                                            egui::Color32::RED
                                        } else if online {
                                            egui::Color32::WHITE
                                        } else {
                                            egui::Color32::GRAY
                                        }),
                                    );
                                    (character_id, selected_client_entity)
                                };

                                if response.clicked() {
                                    if let Some(entity) = selected_client_entity {
                                        selected_target.selected = Some(entity);
                                    }

                                    ui_state.selected_party_member_id = Some(character_id);
                                }
                            }
                        });
                    },
                );
            });

        if player_is_owner {
            if let Some(selected_party_member) =
                ui_state.selected_party_member_id.and_then(|character_id| {
                    party_info
                        .members
                        .iter()
                        .find(|member| member.get_character_id() == character_id)
                })
            {
                if player.character_info.unique_id != selected_party_member.get_character_id() {
                    if response_kick_button.as_ref().map_or(false, |x| x.clicked()) {
                        if let Some(game_connection) = &game_connection {
                            game_connection
                                .client_message_tx
                                .send(ClientMessage::PartyKick {
                                    character_id: selected_party_member.get_character_id(),
                                })
                                .ok();
                        }
                    }

                    if let Some(selected_client_entity_id) =
                        selected_party_member.get_client_entity_id()
                    {
                        if response_entrust_button
                            .as_ref()
                            .map_or(false, |x| x.clicked())
                        {
                            if let Some(game_connection) = &game_connection {
                                game_connection
                                    .client_message_tx
                                    .send(ClientMessage::PartyChangeOwner {
                                        new_owner_entity_id: selected_client_entity_id,
                                    })
                                    .ok();
                            }
                        }
                    }
                }
            }
        }

        if response_leave_button
            .as_ref()
            .map_or(false, |x| x.clicked())
        {
            if let Some(game_connection) = &game_connection {
                game_connection
                    .client_message_tx
                    .send(ClientMessage::PartyLeave)
                    .ok();
            }
        }

        if response_option_button
            .as_ref()
            .map_or(false, |x| x.clicked())
        {
            ui_state_windows.party_options_open = !ui_state_windows.party_options_open;
        }

        if let Some(button) = response_entrust_button {
            button.on_hover_text("Entrust as Leader");
        }

        if let Some(button) = response_kick_button {
            button.on_hover_text("Kick Member");
        }

        if let Some(button) = response_leave_button {
            button.on_hover_text("Leave Party");
        }

        if let Some(button) = response_option_button {
            button.on_hover_text("Party Options");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_party_display_rows, compute_hp_percent, compute_party_member_hp_percent,
        PartyDisplayRow, PARTY_INVITE_BUTTON_HEIGHT, PARTY_INVITE_BUTTON_WIDTH,
    };
    use rose_game_common::{
        components::{HealthPoints, Stamina},
        messages::{
            server::{PartyMemberInfo, PartyMemberInfoOffline, PartyMemberInfoOnline},
            ClientEntityId,
        },
    };

    use crate::components::PartyOwner;

    fn make_member(hp: i32, max_hp: i32) -> PartyMemberInfoOnline {
        PartyMemberInfoOnline {
            character_id: 1,
            name: "test".to_string(),
            entity_id: ClientEntityId(1),
            health_points: HealthPoints::new(hp),
            status_effects: Default::default(),
            max_health: max_hp,
            concentration: 0,
            health_recovery: 0,
            mana_recovery: 0,
            stamina: Stamina::default(),
        }
    }

    fn make_party_member(character_id: u32, name: &str) -> PartyMemberInfo {
        PartyMemberInfo::Offline(PartyMemberInfoOffline {
            character_id,
            name: name.to_string(),
        })
    }

    fn row_character_ids(rows: &[PartyDisplayRow]) -> Vec<u32> {
        rows.iter()
            .map(|row| match row {
                PartyDisplayRow::Player => 99,
                PartyDisplayRow::Member(member) => member.get_character_id(),
            })
            .collect()
    }

    #[test]
    fn hp_percent_uses_party_snapshot_when_max_health_valid() {
        let member_info = make_member(33, 100);
        let hp_percent = compute_party_member_hp_percent(&member_info, Some((1, 10)));
        assert!((hp_percent - 0.33).abs() < f32::EPSILON);
    }

    #[test]
    fn hp_percent_falls_back_when_party_max_health_invalid() {
        let member_info = make_member(33, 0);
        let hp_percent = compute_party_member_hp_percent(&member_info, Some((25, 50)));
        assert!((hp_percent - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn hp_percent_clamps_range() {
        let member_info = make_member(250, 100);
        let hp_percent = compute_party_member_hp_percent(&member_info, None);
        assert!((hp_percent - 1.0).abs() < f32::EPSILON);

        let member_info = make_member(-10, 100);
        let hp_percent = compute_party_member_hp_percent(&member_info, None);
        assert!((hp_percent - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn local_hp_percent_uses_live_player_stats() {
        let hp_percent = compute_hp_percent(45, 90);
        assert!((hp_percent - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn invite_buttons_use_larger_dimensions() {
        assert!(PARTY_INVITE_BUTTON_WIDTH >= 120.0);
        assert!(PARTY_INVITE_BUTTON_HEIGHT >= 36.0);
    }

    #[test]
    fn display_rows_put_player_first_when_player_is_owner() {
        let members = vec![
            make_party_member(10, "member_a"),
            make_party_member(11, "member_b"),
        ];
        let rows = build_party_display_rows(&PartyOwner::Player, &members);

        assert_eq!(row_character_ids(&rows), vec![99, 10, 11]);
    }

    #[test]
    fn display_rows_put_player_after_owner_when_joining_party() {
        let members = vec![
            make_party_member(10, "leader"),
            make_party_member(11, "member_a"),
            make_party_member(12, "member_b"),
        ];
        let rows = build_party_display_rows(&PartyOwner::Character(10), &members);

        assert_eq!(row_character_ids(&rows), vec![10, 99, 11, 12]);
    }

    #[test]
    fn display_rows_fall_back_to_front_when_owner_missing() {
        let members = vec![
            make_party_member(10, "member_a"),
            make_party_member(11, "member_b"),
        ];
        let rows = build_party_display_rows(&PartyOwner::Character(77), &members);

        assert_eq!(row_character_ids(&rows), vec![99, 10, 11]);
    }
}
