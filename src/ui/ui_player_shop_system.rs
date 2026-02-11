use bevy::prelude::{Commands, Entity, Local, Query, Res, ResMut, With};
use bevy_egui::{egui, EguiContexts};
use log::info;
use rose_data::Item;
use rose_game_common::{
    components::{Inventory, InventoryPageType, ItemSlot, INVENTORY_PAGE_SIZE},
    messages::client::ClientMessage,
};

use crate::{
    components::{Command, NextCommand, PersonalStore, PlayerCharacter},
    resources::{GameConnection, GameData},
    ui::UiStateWindows,
};

const PLAYER_SHOP_MAX_SLOTS: usize = 30;

#[derive(Clone)]
struct InventoryEntry {
    item_slot: ItemSlot,
    item: Item,
    item_name: String,
}

struct ShopSetupSlot {
    item_slot: ItemSlot,
    item_name: String,
    quantity: u32,
    max_quantity: u32,
    price: i64,
}

#[derive(Default)]
pub struct UiPlayerShopState {
    title: String,
    debug_buy_slot_index: usize,
    debug_buy_quantity: u32,
    selected_slots: Vec<ShopSetupSlot>,
    last_error: Option<String>,
    last_status: Option<String>,
}

fn inventory_page_code(page_type: InventoryPageType) -> char {
    match page_type {
        InventoryPageType::Equipment => 'E',
        InventoryPageType::Consumables => 'C',
        InventoryPageType::Materials => 'M',
        InventoryPageType::Vehicles => 'V',
    }
}

fn parse_inventory_entries(inventory: &Inventory, game_data: &GameData) -> Vec<InventoryEntry> {
    let mut entries = Vec::new();
    for page_type in [
        InventoryPageType::Equipment,
        InventoryPageType::Consumables,
        InventoryPageType::Materials,
        InventoryPageType::Vehicles,
    ] {
        for index in 0..INVENTORY_PAGE_SIZE {
            let item_slot = ItemSlot::Inventory(page_type, index);
            let Some(item) = inventory.get_item(item_slot).cloned() else {
                continue;
            };
            let item_name = game_data
                .items
                .get_base_item(item.get_item_reference())
                .map(|item_data| item_data.name.to_string())
                .unwrap_or_else(|| String::from("Unknown Item"));

            entries.push(InventoryEntry {
                item_slot,
                item,
                item_name,
            });
        }
    }
    entries
}

fn send_shop_chat_command(game_connection: &Option<Res<GameConnection>>, text: String) -> bool {
    game_connection
        .as_ref()
        .and_then(|connection| {
            connection
                .client_message_tx
                .send(ClientMessage::Chat { text })
                .ok()
        })
        .is_some()
}

pub fn ui_player_shop_system(
    mut commands: Commands,
    mut egui_context: EguiContexts,
    mut ui_state_windows: ResMut<UiStateWindows>,
    mut ui_state: Local<UiPlayerShopState>,
    query_player: Query<(Entity, &Inventory, Option<&PersonalStore>), With<PlayerCharacter>>,
    game_data: Res<GameData>,
    game_connection: Option<Res<GameConnection>>,
) {
    if !ui_state_windows.player_shop_open {
        return;
    }

    let Ok((player_entity, player_inventory, open_personal_store)) = query_player.get_single()
    else {
        return;
    };

    let inventory_entries = parse_inventory_entries(player_inventory, &game_data);
    if ui_state.debug_buy_quantity == 0 {
        ui_state.debug_buy_quantity = 1;
    }

    let mut request_close_window = false;
    egui::Window::new("Player Shop Setup")
        .resizable(true)
        .default_size([980.0, 640.0])
        .open(&mut ui_state_windows.player_shop_open)
        .show(egui_context.ctx_mut(), |ui| {
            ui.horizontal(|ui| {
                let status = if open_personal_store.is_some() {
                    "Open"
                } else {
                    "Closed"
                };
                ui.label(format!("Shop status: {}", status));
                if let Some(personal_store) = open_personal_store {
                    ui.label(format!("Title: {}", personal_store.title));
                }
            });

            ui.horizontal(|ui| {
                ui.label("Title:");
                ui.text_edit_singleline(&mut ui_state.title);
            });

            if let Some(error) = ui_state.last_error.as_ref() {
                ui.colored_label(egui::Color32::RED, error);
            }
            if let Some(status) = ui_state.last_status.as_ref() {
                ui.colored_label(egui::Color32::LIGHT_GREEN, status);
            }

            ui.separator();

            ui.columns(2, |columns| {
                columns[0].heading("Inventory");
                columns[0].label("Select items to add to the shop.");
                egui::ScrollArea::vertical()
                    .id_source("player_shop_inventory_scroll")
                    .show(&mut columns[0], |ui| {
                        for entry in inventory_entries.iter() {
                            let already_selected = ui_state
                                .selected_slots
                                .iter()
                                .any(|slot| slot.item_slot == entry.item_slot);
                            ui.horizontal(|ui| {
                                let slot_label = match entry.item_slot {
                                    ItemSlot::Inventory(page_type, index) => {
                                        format!("{}{}", inventory_page_code(page_type), index)
                                    }
                                    _ => String::from("-"),
                                };
                                ui.add_sized([56.0, 20.0], egui::Label::new(slot_label));
                                ui.add_sized(
                                    [90.0, 20.0],
                                    egui::Label::new(format!("x{}", entry.item.get_quantity())),
                                );
                                ui.add_sized([250.0, 20.0], egui::Label::new(&entry.item_name));
                                if ui
                                    .add_enabled(
                                        !already_selected
                                            && ui_state.selected_slots.len()
                                                < PLAYER_SHOP_MAX_SLOTS,
                                        egui::Button::new("Add"),
                                    )
                                    .clicked()
                                {
                                    ui_state.selected_slots.push(ShopSetupSlot {
                                        item_slot: entry.item_slot,
                                        item_name: entry.item_name.clone(),
                                        quantity: 1,
                                        max_quantity: entry.item.get_quantity(),
                                        price: 1,
                                    });
                                }
                            });
                        }
                    });

                columns[1].heading("Shop Listings");
                columns[1].label(format!(
                    "Selected slots: {}/{}",
                    ui_state.selected_slots.len(),
                    PLAYER_SHOP_MAX_SLOTS
                ));
                egui::ScrollArea::vertical()
                    .id_source("player_shop_selected_scroll")
                    .show(&mut columns[1], |ui| {
                        let mut remove_index = None;
                        for (index, selected) in ui_state.selected_slots.iter_mut().enumerate() {
                            ui.horizontal(|ui| {
                                let slot_label = match selected.item_slot {
                                    ItemSlot::Inventory(page_type, slot_index) => {
                                        format!("{}{}", inventory_page_code(page_type), slot_index)
                                    }
                                    _ => String::from("-"),
                                };
                                ui.add_sized([56.0, 20.0], egui::Label::new(slot_label));
                                ui.add_sized([210.0, 20.0], egui::Label::new(&selected.item_name));

                                ui.label("Qty");
                                ui.add(
                                    egui::DragValue::new(&mut selected.quantity)
                                        .speed(1.0)
                                        .clamp_range(1..=selected.max_quantity),
                                );

                                ui.label("Price");
                                ui.add(
                                    egui::DragValue::new(&mut selected.price)
                                        .speed(1.0)
                                        .clamp_range(0..=i64::MAX),
                                );

                                if ui.button("Remove").clicked() {
                                    remove_index = Some(index);
                                }
                            });
                        }

                        if let Some(index) = remove_index {
                            ui_state.selected_slots.remove(index);
                        }
                    });
            });

            ui.separator();

            ui.horizontal(|ui| {
                if ui.button("Open Shop").clicked() {
                    ui_state.last_error = None;
                    ui_state.last_status = None;

                    if ui_state.selected_slots.is_empty() {
                        ui_state.last_error =
                            Some(String::from("Shop must have at least one listing."));
                        return;
                    }

                    for selected in ui_state.selected_slots.iter() {
                        let Some(item) = player_inventory.get_item(selected.item_slot) else {
                            ui_state.last_error = Some(format!(
                                "Inventory slot {:?} is now empty.",
                                selected.item_slot
                            ));
                            return;
                        };
                        if selected.quantity == 0 || selected.quantity > item.get_quantity() {
                            ui_state.last_error = Some(format!(
                                "Invalid quantity for {} (max {}).",
                                selected.item_name,
                                item.get_quantity()
                            ));
                            return;
                        }
                        if selected.price < 0 {
                            ui_state.last_error = Some(format!(
                                "Price for {} must be non-negative.",
                                selected.item_name
                            ));
                            return;
                        }
                    }

                    let title = if ui_state.title.trim().is_empty() {
                        String::from("My Shop")
                    } else {
                        ui_state.title.trim().replace('"', "")
                    };
                    let listings = ui_state
                        .selected_slots
                        .iter()
                        .filter_map(|slot| {
                            if let ItemSlot::Inventory(page_type, slot_index) = slot.item_slot {
                                Some(format!(
                                    "{}:{}:{}:{}",
                                    inventory_page_code(page_type),
                                    slot_index,
                                    slot.quantity,
                                    slot.price
                                ))
                            } else {
                                None
                            }
                        })
                        .collect::<Vec<_>>()
                        .join(";");

                    let command = format!("/pshop_open \"{}\" \"{}\"", title, listings);
                    if send_shop_chat_command(&game_connection, command.clone()) {
                        info!(
                            "player-shop: open requested with {} slot(s)",
                            ui_state.selected_slots.len()
                        );
                        ui_state.last_status = Some(String::from("Shop open request sent."));
                    } else {
                        ui_state.last_error =
                            Some(String::from("Failed to send shop open request."));
                    }
                }

                if ui.button("Close Shop").clicked() {
                    let command = String::from("/pshop_close");
                    if send_shop_chat_command(&game_connection, command) {
                        info!("player-shop: close requested");
                        // Optimistically clear local state so movement/model recover immediately
                        // even if the authoritative close packet is delayed.
                        commands
                            .entity(player_entity)
                            .remove::<PersonalStore>()
                            .insert(Command::with_stop())
                            .insert(NextCommand::with_stop());
                        ui_state.last_status = Some(String::from("Shop close request sent."));
                    } else {
                        ui_state.last_error =
                            Some(String::from("Failed to send shop close request."));
                    }
                }

                ui.separator();
                ui.label("Debug Buy Slot:");
                ui.add(
                    egui::DragValue::new(&mut ui_state.debug_buy_slot_index)
                        .speed(1.0)
                        .clamp_range(0..=PLAYER_SHOP_MAX_SLOTS - 1),
                );
                ui.label("Qty:");
                ui.add(
                    egui::DragValue::new(&mut ui_state.debug_buy_quantity)
                        .speed(1.0)
                        .clamp_range(1..=999u32),
                );
                if ui.button("Debug Buy").clicked() {
                    let command = format!(
                        "/pshop_test_buy {} {}",
                        ui_state.debug_buy_slot_index, ui_state.debug_buy_quantity
                    );
                    if send_shop_chat_command(&game_connection, command.clone()) {
                        info!(
                            "player-shop: debug buy requested slot={} qty={}",
                            ui_state.debug_buy_slot_index, ui_state.debug_buy_quantity
                        );
                        ui_state.last_status = Some(format!(
                            "Debug buy requested for slot {} x{}",
                            ui_state.debug_buy_slot_index, ui_state.debug_buy_quantity
                        ));
                    } else {
                        ui_state.last_error =
                            Some(String::from("Failed to send debug buy request."));
                    }
                }
            });

            if ui.button("Close Window").clicked() {
                request_close_window = true;
            }
        });

    if request_close_window {
        ui_state_windows.player_shop_open = false;
    }
}
