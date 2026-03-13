use bevy::{
    ecs::query::WorldQuery,
    prelude::{Assets, EventWriter, Local, Query, Res, ResMut, With},
};
use bevy_egui::{egui, EguiContexts};
use rose_data::ClanMemberPosition;

use rose_game_common::{
    components::{
        AbilityValues, BasicStatType, BasicStats, CharacterInfo, ExperiencePoints, Level,
        MoveSpeed, Stamina, StatPoints, UnionMembership, MAX_STAMINA,
    },
    messages::client::ClientMessage,
};

use crate::{
    components::{ClanMembership, PlayerCharacter},
    resources::{GameConnection, GameData, UiResources},
    ui::{
        widgets::{DataBindings, Dialog, DrawText},
        UiSoundEvent, UiStateWindows,
    },
};

const IID_BTN_CLOSE: i32 = 10;
// const IID_BTN_DIALOG2ICON: i32 = 11;
const IID_TABBEDPANE: i32 = 20;
const IID_TAB_BASICINFO: i32 = 21;
// const IID_TAB_BASICINFO_BG: i32 = 22;
// const IID_TAB_BASICINFO_BTN: i32 = 23;
const IID_GUAGE_STAMINA: i32 = 24;
const IID_TAB_ABILITY: i32 = 31;
// const IID_TAB_ABILITY_BG: i32 = 32;
// const IID_TAB_ABILITY_BTN: i32 = 33;
const IID_BTN_UP_STR: i32 = 34;
const IID_BTN_UP_DEX: i32 = 35;
const IID_BTN_UP_INT: i32 = 36;
const IID_BTN_UP_CON: i32 = 37;
const IID_BTN_UP_CHARM: i32 = 38;
const IID_BTN_UP_SENSE: i32 = 39;
const IID_TAB_UNION: i32 = 41;
// const IID_TAB_UNION_BG: i32 = 42;
// const IID_TAB_UNION_BTN: i32 = 43;
const UNION_STB_COLOR_COLUMN: usize = 1;
const UNION_STB_STRING_ID_COLUMN: usize = 11;
const UNION_COLORS: [egui::Color32; 10] = [
    egui::Color32::from_rgb(255, 0, 0),     // red
    egui::Color32::from_rgb(0, 255, 0),     // green
    egui::Color32::from_rgb(0, 0, 255),     // blue
    egui::Color32::from_rgb(0, 0, 0),       // black
    egui::Color32::from_rgb(255, 255, 255), // white
    egui::Color32::from_rgb(255, 255, 0),   // yellow
    egui::Color32::from_rgb(150, 150, 150), // gray
    egui::Color32::from_rgb(255, 0, 255),   // violet
    egui::Color32::from_rgb(255, 128, 0),   // orange
    egui::Color32::from_rgb(255, 136, 200), // pink
];

pub struct UiStateCharacterInfo {
    current_tab: i32,
}

impl Default for UiStateCharacterInfo {
    fn default() -> Self {
        Self {
            current_tab: IID_TAB_BASICINFO,
        }
    }
}

fn clan_position_name(game_data: &GameData, position: ClanMemberPosition) -> String {
    let clan_position_name = game_data
        .string_database
        .get_clan_member_position(position)
        .trim();
    if !clan_position_name.is_empty() {
        return clan_position_name.to_string();
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

#[derive(WorldQuery)]
pub struct PlayerQuery<'w> {
    ability_values: &'w AbilityValues,
    basic_stats: &'w BasicStats,
    clan_membership: Option<&'w ClanMembership>,
    character_info: &'w CharacterInfo,
    experience_points: &'w ExperiencePoints,
    level: &'w Level,
    move_speed: &'w MoveSpeed,
    stamina: &'w Stamina,
    stat_points: &'w StatPoints,
    union_membership: &'w UnionMembership,
}

pub fn ui_character_info_system(
    mut egui_context: EguiContexts,
    query_player: Query<PlayerQuery, With<PlayerCharacter>>,
    mut ui_state: Local<UiStateCharacterInfo>,
    mut ui_state_windows: ResMut<UiStateWindows>,
    mut ui_sound_events: EventWriter<UiSoundEvent>,
    ui_resources: Res<UiResources>,
    dialog_assets: Res<Assets<Dialog>>,
    game_connection: Option<Res<GameConnection>>,
    game_data: Res<GameData>,
) {
    let dialog = if let Some(dialog) = dialog_assets.get(&ui_resources.dialog_character_info) {
        dialog
    } else {
        return;
    };

    let player = if let Ok(player) = query_player.get_single() {
        player
    } else {
        return;
    };

    let ui_state = &mut *ui_state;
    let mut response_close_button = None;
    let mut response_raise_str_button = None;
    let mut response_raise_dex_button = None;
    let mut response_raise_int_button = None;
    let mut response_raise_con_button = None;
    let mut response_raise_cha_button = None;
    let mut response_raise_sen_button = None;

    egui::Window::new("Character Info")
        .frame(egui::Frame::none())
        .open(&mut ui_state_windows.character_info_open)
        .title_bar(false)
        .resizable(false)
        .default_width(dialog.width)
        .default_height(dialog.height)
        .show(egui_context.ctx_mut(), |ui| {
            let need_xp = game_data
                .ability_value_calculator
                .calculate_levelup_require_xp(player.level.level);
            let stamina = player.stamina.stamina as f32 / MAX_STAMINA as f32;

            dialog.draw(
                ui,
                DataBindings {
                    sound_events: Some(&mut ui_sound_events),
                    response: &mut [
                        (IID_BTN_CLOSE, &mut response_close_button),
                        (IID_BTN_UP_STR, &mut response_raise_str_button),
                        (IID_BTN_UP_DEX, &mut response_raise_dex_button),
                        (IID_BTN_UP_INT, &mut response_raise_int_button),
                        (IID_BTN_UP_CON, &mut response_raise_con_button),
                        (IID_BTN_UP_CHARM, &mut response_raise_cha_button),
                        (IID_BTN_UP_SENSE, &mut response_raise_sen_button),
                    ],
                    gauge: &mut [(
                        IID_GUAGE_STAMINA,
                        &stamina,
                        &format!("{} / {}", player.stamina.stamina, MAX_STAMINA),
                    )],
                    tabs: &mut [(IID_TABBEDPANE, &mut ui_state.current_tab)],
                    ..Default::default()
                },
                |ui, bindings| match bindings.get_tab(IID_TABBEDPANE) {
                    Some(&mut IID_TAB_BASICINFO) => {
                        let clan_name = player
                            .clan_membership
                            .map(|clan_membership| clan_membership.name.trim())
                            .filter(|name| !name.is_empty())
                            .unwrap_or("-");
                        let clan_rank = player.clan_membership.map_or_else(
                            || "-".to_string(),
                            |clan_membership| {
                                clan_position_name(&game_data, clan_membership.position)
                            },
                        );

                        ui.add_label_at(egui::pos2(59.0, 67.0), &player.character_info.name);
                        ui.add_label_at(
                            egui::pos2(59.0, 88.0),
                            game_data
                                .string_database
                                .get_job_name(player.character_info.job),
                        );
                        ui.add_label_at(egui::pos2(59.0, 109.0), clan_name);
                        ui.add_label_at(egui::pos2(59.0, 130.0), clan_rank);
                        ui.add_label_at(
                            egui::pos2(59.0, 172.0),
                            &format!("{}", player.level.level),
                        );
                        ui.add_label_at(
                            egui::pos2(59.0, 193.0),
                            &format!("{} / {}", player.experience_points.xp, need_xp),
                        );
                    }
                    Some(&mut IID_TAB_ABILITY) => {
                        ui.add_label_at(
                            egui::pos2(58.0, 67.0),
                            &format!("{}", player.ability_values.get_strength()),
                        );
                        ui.add_label_at(
                            egui::pos2(58.0, 88.0),
                            &format!("{}", player.ability_values.get_dexterity()),
                        );
                        ui.add_label_at(
                            egui::pos2(58.0, 109.0),
                            &format!("{}", player.ability_values.get_intelligence()),
                        );
                        ui.add_label_at(
                            egui::pos2(58.0, 130.0),
                            &format!("{}", player.ability_values.get_concentration()),
                        );
                        ui.add_label_at(
                            egui::pos2(58.0, 151.0),
                            &format!("{}", player.ability_values.get_charm()),
                        );
                        ui.add_label_at(
                            egui::pos2(58.0, 172.0),
                            &format!("{}", player.ability_values.get_sense()),
                        );
                        ui.add_label_at(
                            egui::pos2(69.0, 211.0),
                            &format!("{}", player.stat_points.points),
                        );

                        ui.add_label_at(
                            egui::pos2(171.0, 67.0),
                            &format!("{}", player.ability_values.get_attack_power()),
                        );
                        ui.add_label_at(
                            egui::pos2(171.0, 88.0),
                            &format!("{}", player.ability_values.get_defence()),
                        );
                        ui.add_label_at(
                            egui::pos2(171.0, 109.0),
                            &format!("{}", player.ability_values.get_resistance()),
                        );
                        ui.add_label_at(
                            egui::pos2(171.0, 130.0),
                            &format!("{}", player.ability_values.get_hit()),
                        );
                        ui.add_label_at(
                            egui::pos2(171.0, 151.0),
                            &format!("{}", player.ability_values.get_critical()),
                        );
                        ui.add_label_at(
                            egui::pos2(171.0, 172.0),
                            &format!("{}", player.ability_values.get_avoid()),
                        );
                        ui.add_label_at(
                            egui::pos2(171.0, 193.0),
                            &format!("{}", player.ability_values.get_attack_speed()),
                        );
                        ui.add_label_at(
                            egui::pos2(171.0, 214.0),
                            &format!("{}", player.move_speed.speed),
                        );
                    }
                    Some(&mut IID_TAB_UNION) => {
                        let (union_name, union_name_color) = player
                            .union_membership
                            .current_union
                            .and_then(|current_union| {
                                let union_id = current_union.get();
                                let union_string_key = game_data
                                    .stb_union
                                    .try_get(union_id, UNION_STB_STRING_ID_COLUMN)?;
                                let union_name = game_data
                                    .string_database
                                    .union
                                    .get_text_string(
                                        game_data.string_database.language,
                                        union_string_key,
                                    )
                                    .filter(|name| !name.is_empty())?;
                                let union_name_color = game_data
                                    .stb_union
                                    .try_get_int(union_id, UNION_STB_COLOR_COLUMN)
                                    .and_then(|color_index| {
                                        UNION_COLORS.get(color_index as usize).copied()
                                    })
                                    .unwrap_or(egui::Color32::WHITE);
                                Some((union_name, union_name_color))
                            })
                            .unwrap_or(("-", egui::Color32::WHITE));

                        ui.add_label_at(
                            egui::pos2(90.0, 67.0),
                            egui::RichText::new(union_name).color(union_name_color),
                        );

                        for column in 0..2 {
                            let x = 50.0 + (column as f32 * 113.0);
                            for row in 0..5 {
                                let index = column * 5 + row;
                                let y = 130.0 + (row as f32 * 21.0);
                                ui.add_label_at(
                                    egui::pos2(x, y),
                                    format!("{}", player.union_membership.points[index]),
                                );
                            }
                        }
                    }
                    _ => {}
                },
            );
        });

    if response_close_button.map_or(false, |r| r.clicked()) {
        ui_state_windows.character_info_open = false;
    }

    let stat_button_response = |basic_stat_type: BasicStatType,
                                response: Option<egui::Response>| {
        if let Some(response) = response {
            if let Some(cost) = game_data
                .ability_value_calculator
                .calculate_basic_stat_increase_cost(player.basic_stats, basic_stat_type)
            {
                if response
                    .on_hover_text(format!("Required Points: {}", cost))
                    .clicked()
                    && cost <= player.stat_points.points
                {
                    if let Some(game_connection) = game_connection.as_ref() {
                        game_connection
                            .client_message_tx
                            .send(ClientMessage::IncreaseBasicStat { basic_stat_type })
                            .ok();
                    }
                }
            }
        }
    };

    stat_button_response(BasicStatType::Strength, response_raise_str_button);
    stat_button_response(BasicStatType::Dexterity, response_raise_dex_button);
    stat_button_response(BasicStatType::Intelligence, response_raise_int_button);
    stat_button_response(BasicStatType::Concentration, response_raise_con_button);
    stat_button_response(BasicStatType::Charm, response_raise_cha_button);
    stat_button_response(BasicStatType::Sense, response_raise_sen_button);
}
