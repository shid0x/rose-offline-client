use bevy::prelude::{Input, KeyCode, Local, Res, ResMut};
use bevy_egui::{egui, EguiContexts};

use rose_data::ItemType;
use rose_data_irose::encode_item_type;
use rose_game_common::messages::client::ClientMessage;

use crate::{
    resources::{GameConnection, GameData},
    ui::UiStateWindows,
};

const ITEM_TYPES: [ItemType; 14] = [
    ItemType::Face,
    ItemType::Head,
    ItemType::Body,
    ItemType::Hands,
    ItemType::Feet,
    ItemType::Back,
    ItemType::Jewellery,
    ItemType::Weapon,
    ItemType::SubWeapon,
    ItemType::Consumable,
    ItemType::Gem,
    ItemType::Material,
    ItemType::Quest,
    ItemType::Vehicle,
];

#[derive(Clone)]
struct BrowserItemRow {
    item_type: ItemType,
    item_id: usize,
    item_name: String,
}

pub struct UiStateItemBrowser {
    filter_item_type: Option<ItemType>,
    search_text: String,
    quantity: usize,
    socket: bool,
    gem: usize,
    grade: u8,
    filtered_items: Vec<BrowserItemRow>,
    last_status: Option<String>,
    last_search_key: String,
}

impl Default for UiStateItemBrowser {
    fn default() -> Self {
        Self {
            filter_item_type: None,
            search_text: String::new(),
            quantity: 1,
            socket: false,
            gem: 0,
            grade: 0,
            filtered_items: Vec::new(),
            last_status: None,
            last_search_key: String::new(),
        }
    }
}

fn refresh_item_results(ui_state: &mut UiStateItemBrowser, game_data: &GameData) {
    let query = ui_state.search_text.to_ascii_lowercase();
    let filter_key = ui_state
        .filter_item_type
        .map(|item_type| format!("{:?}", item_type))
        .unwrap_or_else(|| String::from("ALL"));
    ui_state.last_search_key = format!("{}|{}", query, filter_key);

    let mut rows = Vec::new();
    for item_type in ITEM_TYPES {
        if ui_state
            .filter_item_type
            .map_or(false, |filter_type| filter_type != item_type)
        {
            continue;
        }

        for item_reference in game_data.items.iter_items(item_type) {
            let Some(item_data) = game_data.items.get_base_item(item_reference) else {
                continue;
            };

            let item_name = item_data.name.trim();
            if item_name.is_empty() {
                continue;
            }

            if !query.is_empty() && !item_name.to_ascii_lowercase().contains(&query) {
                continue;
            }

            rows.push(BrowserItemRow {
                item_type,
                item_id: item_reference.item_number,
                item_name: item_name.to_string(),
            });
        }
    }

    ui_state.filtered_items = rows;
}

pub fn ui_item_browser_system(
    mut egui_context: EguiContexts,
    keyboard_input: Res<Input<KeyCode>>,
    mut ui_state_windows: ResMut<UiStateWindows>,
    mut ui_state_item_browser: Local<UiStateItemBrowser>,
    game_connection: Option<Res<GameConnection>>,
    game_data: Res<GameData>,
) {
    if keyboard_input.just_pressed(KeyCode::F9) {
        ui_state_windows.item_browser_open = !ui_state_windows.item_browser_open;
    }

    if !ui_state_windows.item_browser_open {
        return;
    }

    let current_filter_key = ui_state_item_browser
        .filter_item_type
        .map(|item_type| format!("{:?}", item_type))
        .unwrap_or_else(|| String::from("ALL"));
    if ui_state_item_browser.last_search_key
        != format!(
            "{}|{}",
            ui_state_item_browser.search_text.to_ascii_lowercase(),
            current_filter_key
        )
    {
        refresh_item_results(&mut ui_state_item_browser, &game_data);
    }

    egui::Window::new("Item Browser")
        .resizable(true)
        .default_size([780.0, 520.0])
        .open(&mut ui_state_windows.item_browser_open)
        .show(egui_context.ctx_mut(), |ui| {
            ui.horizontal(|ui| {
                ui.label("Search:");
                if ui
                    .text_edit_singleline(&mut ui_state_item_browser.search_text)
                    .changed()
                {
                    refresh_item_results(&mut ui_state_item_browser, &game_data);
                }
                ui.separator();
                ui.label("Type:");
                let previous_filter = ui_state_item_browser.filter_item_type;
                egui::ComboBox::from_id_source("item_browser_type_filter")
                    .selected_text(
                        ui_state_item_browser
                            .filter_item_type
                            .map(|item_type| format!("{:?}", item_type))
                            .unwrap_or_else(|| String::from("All")),
                    )
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut ui_state_item_browser.filter_item_type,
                            None,
                            "All",
                        );
                        for item_type in ITEM_TYPES {
                            ui.selectable_value(
                                &mut ui_state_item_browser.filter_item_type,
                                Some(item_type),
                                format!("{:?}", item_type),
                            );
                        }
                    });
                if ui_state_item_browser.filter_item_type != previous_filter {
                    refresh_item_results(&mut ui_state_item_browser, &game_data);
                }
                ui.label(format!("Matches: {}", ui_state_item_browser.filtered_items.len()));
            });

            ui.separator();

            ui.horizontal(|ui| {
                ui.label("Quantity:");
                ui.add(
                    egui::DragValue::new(&mut ui_state_item_browser.quantity)
                        .speed(1.0)
                        .clamp_range(1..=999usize),
                );
                ui.label("Socket:");
                ui.checkbox(&mut ui_state_item_browser.socket, "");
                ui.label("Gem:");
                ui.add(
                    egui::DragValue::new(&mut ui_state_item_browser.gem)
                        .speed(1.0)
                        .clamp_range(0..=9999usize),
                );
                ui.label("Grade:");
                ui.add(
                    egui::DragValue::new(&mut ui_state_item_browser.grade)
                        .speed(1.0)
                        .clamp_range(0..=9u8),
                );
            });

            if let Some(status) = ui_state_item_browser.last_status.as_ref() {
                ui.label(status);
            }

            ui.separator();

            ui.horizontal(|ui| {
                ui.strong("Type");
                ui.add_space(54.0);
                ui.strong("ID");
                ui.add_space(56.0);
                ui.strong("Name");
                ui.add_space(240.0);
                ui.strong("Action");
            });
            ui.separator();

            let row_height = 24.0;
            let row_count = ui_state_item_browser.filtered_items.len();
            let mut pending_status: Option<String> = None;

            ui.style_mut().spacing.item_spacing.y = 2.0;
            ui.set_min_width(740.0);
            egui::ScrollArea::vertical().show_rows(ui, row_height, row_count, |ui, row_range| {
                for row_index in row_range {
                    let row = &ui_state_item_browser.filtered_items[row_index];
                    ui.horizontal(|ui| {
                        ui.add_sized(
                            [110.0, row_height],
                            egui::Label::new(format!("{:?}", row.item_type)),
                        );
                        ui.add_sized(
                            [80.0, row_height],
                            egui::Label::new(row.item_id.to_string()),
                        );
                        ui.add_sized([420.0, row_height], egui::Label::new(&row.item_name));

                        let can_send = game_connection.is_some();
                        if ui
                            .add_enabled(
                                can_send,
                                egui::Button::new("Give").min_size(egui::vec2(58.0, row_height)),
                            )
                            .clicked()
                        {
                            let Some(item_type_code) = encode_item_type(row.item_type) else {
                                pending_status = Some(format!(
                                    "Failed: cannot encode item type {:?}",
                                    row.item_type
                                ));
                                return;
                            };

                            let command = format!(
                                "/item {} {} {} {} {} {}",
                                item_type_code,
                                row.item_id,
                                ui_state_item_browser.quantity,
                                if ui_state_item_browser.socket { 1 } else { 0 },
                                ui_state_item_browser.gem,
                                ui_state_item_browser.grade
                            );

                            let send_result = game_connection
                                .as_ref()
                                .and_then(|connection| {
                                    connection
                                        .client_message_tx
                                        .send(ClientMessage::Chat {
                                            text: command.clone(),
                                        })
                                        .ok()
                                })
                                .is_some();

                            pending_status = Some(if send_result {
                                format!("Sent: {}", command)
                            } else {
                                String::from("Failed: not connected")
                            });
                        }
                    });
                }
            });

            if let Some(status) = pending_status {
                ui_state_item_browser.last_status = Some(status);
            }
        });
}
