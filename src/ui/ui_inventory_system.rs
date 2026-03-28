use bevy::{
    ecs::query::WorldQuery,
    prelude::{Assets, EventWriter, Events, Local, Query, Res, ResMut, With, World},
};
use bevy_egui::{egui, EguiContexts};
use enum_map::{enum_map, EnumMap};

use rose_data::{
    AmmoIndex, EquipmentIndex, Item, ItemClass, ItemType, VehiclePartIndex, VehicleType,
};
use rose_game_common::components::{
    AbilityValues, Equipment, Inventory, InventoryPageType, ItemSlot, INVENTORY_PAGE_SIZE,
};

use crate::{
    components::{Cooldowns, PlayerCharacter},
    events::{NumberInputDialogEvent, PersonalStoreEvent, PlayerCommandEvent},
    resources::{GameData, UiResources},
    ui::{
        tooltips::{PlayerTooltipQuery, PlayerTooltipQueryItem},
        ui_add_item_tooltip,
        widgets::{DataBindings, Dialog, Widget},
        DialogInstance, DragAndDropId, DragAndDropSlot, UiSoundEvent, UiStateDragAndDrop,
        UiStateWindows,
    },
};

const IID_BTN_CLOSE: i32 = 10;
// const IID_BTN_ICONIZE: i32 = 11;
const IID_BTN_MONEY: i32 = 12;
const IID_TABBEDPANE_EQUIP: i32 = 20;
const IID_TAB_EQUIP_PAT: i32 = 21;
// const IID_BTN_EQUIP_PAT: i32 = 23;
const IID_TAB_EQUIP_AVATAR: i32 = 31;
// const IID_BTN_EQUIP_AVATAR: i32 = 33;
const IID_TABBEDPANE_INVEN_ITEM: i32 = 50;
const IID_TAB_INVEN_EQUIP: i32 = 51;
// const IID_BTN_INVEN_EQUIP: i32 = 53;
const IID_TAB_INVEN_USE: i32 = 61;
// const IID_BTN_INVEN_USE: i32 = 63;
const IID_TAB_INVEN_ETC: i32 = 71;
// const IID_BTN_INVEN_ETC: i32 = 73;
const IID_TABBEDPANE_INVEN_PAT: i32 = 100;
const IID_TAB_INVEN_PAT: i32 = 101;
// const IID_PANE_EQUIP: i32 = 200;
const IID_BTN_MINIMIZE: i32 = 213;
const IID_BTN_MAXIMIZE: i32 = 214;
const IID_PANE_INVEN: i32 = 300;

pub struct UiStateInventory {
    dialog_instance: DialogInstance,
    item_slot_map: EnumMap<InventoryPageType, Vec<ItemSlot>>,
    current_equipment_tab: i32,
    current_vehicle_tab: i32,
    current_inventory_tab: i32,
    minimised: bool,
}

impl Default for UiStateInventory {
    fn default() -> Self {
        Self {
            dialog_instance: DialogInstance::new("DLGITEM.XML"),
            item_slot_map: enum_map! {
                page_type => (0..INVENTORY_PAGE_SIZE)
                .map(|index| ItemSlot::Inventory(page_type, index))
                .collect(),
            },
            current_equipment_tab: IID_TAB_EQUIP_AVATAR,
            current_vehicle_tab: IID_TAB_INVEN_PAT,
            current_inventory_tab: IID_TAB_INVEN_EQUIP,
            minimised: false,
        }
    }
}

const EQUIPMENT_GRID_SLOTS: [(rose_game_common::components::ItemSlot, egui::Pos2); 14] = [
    (
        ItemSlot::Equipment(EquipmentIndex::Face),
        egui::pos2(19.0, 67.0),
    ),
    (
        ItemSlot::Equipment(EquipmentIndex::Head),
        egui::pos2(69.0, 67.0),
    ),
    (
        ItemSlot::Equipment(EquipmentIndex::Back),
        egui::pos2(119.0, 67.0),
    ),
    (ItemSlot::Ammo(AmmoIndex::Arrow), egui::pos2(169.0, 67.0)),
    (
        ItemSlot::Equipment(EquipmentIndex::Weapon),
        egui::pos2(19.0, 113.0),
    ),
    (
        ItemSlot::Equipment(EquipmentIndex::Body),
        egui::pos2(69.0, 113.0),
    ),
    (
        ItemSlot::Equipment(EquipmentIndex::SubWeapon),
        egui::pos2(119.0, 113.0),
    ),
    (ItemSlot::Ammo(AmmoIndex::Bullet), egui::pos2(169.0, 113.0)),
    (
        ItemSlot::Equipment(EquipmentIndex::Hands),
        egui::pos2(19.0, 159.0),
    ),
    (
        ItemSlot::Equipment(EquipmentIndex::Feet),
        egui::pos2(69.0, 159.0),
    ),
    (ItemSlot::Ammo(AmmoIndex::Throw), egui::pos2(169.0, 159.0)),
    (
        ItemSlot::Equipment(EquipmentIndex::Ring),
        egui::pos2(19.0, 205.0),
    ),
    (
        ItemSlot::Equipment(EquipmentIndex::Necklace),
        egui::pos2(69.0, 205.0),
    ),
    (
        ItemSlot::Equipment(EquipmentIndex::Earring),
        egui::pos2(119.0, 205.0),
    ),
];

const VEHICLE_GRID_SLOTS: [(rose_game_common::components::ItemSlot, egui::Pos2); 4] = [
    (
        ItemSlot::Vehicle(VehiclePartIndex::Body),
        egui::pos2(19.0, 68.0),
    ),
    (
        ItemSlot::Vehicle(VehiclePartIndex::Engine),
        egui::pos2(19.0, 114.0),
    ),
    (
        ItemSlot::Vehicle(VehiclePartIndex::Leg),
        egui::pos2(19.0, 160.0),
    ),
    (
        ItemSlot::Vehicle(VehiclePartIndex::Arms),
        egui::pos2(19.0, 206.0),
    ),
];

const TUNING_STAT_VALUE_X: f32 = 153.0;
const TUNING_TYPE_VALUE_Y: f32 = 67.0;
const TUNING_DEF_VALUE_Y: f32 = 89.0;
const TUNING_MDEF_VALUE_Y: f32 = 113.0;
const TUNING_FUEL_VALUE_Y: f32 = 135.0;
const TUNING_SPEED_VALUE_Y: f32 = 159.0;
const TUNING_ATK_VALUE_Y: f32 = 181.0;
const TUNING_ASPD_VALUE_Y: f32 = 203.0;
const TUNING_STAT_VALUE_WIDTH: f32 = 58.0;
const TUNING_TYPE_VALUE_WIDTH: f32 = 62.0;
const TUNING_STAT_VALUE_HEIGHT: f32 = 16.0;
const TUNING_STAT_TEXT_COLOR: egui::Color32 = egui::Color32::from_rgb(255, 245, 214);
const TUNING_STAT_FONT_SIZE: f32 = 10.0;
const TUNING_TYPE_FONT_SIZE: f32 = 11.0;

#[derive(Clone, Debug, PartialEq, Eq)]
struct VehicleTuningStats {
    vehicle_type_label: &'static str,
    defence: i32,
    magic_defence: i32,
    fuel_consumption: u32,
    speed: i32,
    attack: i32,
    attack_speed: i32,
}

impl VehicleTuningStats {
    fn from_sources(
        equipment: &Equipment,
        ability_values: &AbilityValues,
        game_data: &GameData,
    ) -> Self {
        let body_vehicle_type = equipment
            .get_vehicle_item(VehiclePartIndex::Body)
            .and_then(|item| game_data.items.get_vehicle_item(item.item.item_number))
            .map(|item_data| item_data.vehicle_type);
        let engine_fuel_use_rate = equipment
            .get_vehicle_item(VehiclePartIndex::Engine)
            .and_then(|item| game_data.items.get_vehicle_item(item.item.item_number))
            .map(|item_data| item_data.fuel_use_rate);

        Self::from_resolved_parts(body_vehicle_type, engine_fuel_use_rate, ability_values)
    }

    fn from_resolved_parts(
        body_vehicle_type: Option<VehicleType>,
        engine_fuel_use_rate: Option<u32>,
        ability_values: &AbilityValues,
    ) -> Self {
        let Some(body_vehicle_type) = body_vehicle_type else {
            return Self {
                vehicle_type_label: "-",
                defence: 0,
                magic_defence: 0,
                fuel_consumption: 0,
                speed: 0,
                attack: 0,
                attack_speed: 0,
            };
        };

        Self {
            vehicle_type_label: match body_vehicle_type {
                VehicleType::Cart => "Cart",
                VehicleType::CastleGear => "Castle Gear",
            },
            defence: (ability_values.vehicle_defence + ability_values.adjust.defence).max(0),
            magic_defence: (ability_values.resistance + ability_values.adjust.resistance).max(0),
            fuel_consumption: engine_fuel_use_rate.unwrap_or(0),
            speed: (ability_values.vehicle_move_speed + ability_values.adjust.run_speed)
                .max(0.0)
                .round() as i32,
            attack: (ability_values.vehicle_attack_power + ability_values.adjust.attack_power)
                .max(0),
            attack_speed: (ability_values.vehicle_attack_speed
                + ability_values.adjust.attack_speed)
                .max(0),
        }
    }
}

fn draw_tuning_stat_text(ui: &egui::Ui, rect: egui::Rect, text: &str, font_size: f32) {
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        text,
        egui::FontId::proportional(font_size),
        TUNING_STAT_TEXT_COLOR,
    );
}

fn draw_vehicle_tuning_stats(ui: &egui::Ui, stats: &VehicleTuningStats) {
    let min = ui.min_rect().min;
    let draw_rect = |x: f32, y: f32, width: f32| {
        egui::Rect::from_min_size(
            min + egui::vec2(x, y),
            egui::vec2(width, TUNING_STAT_VALUE_HEIGHT),
        )
    };

    draw_tuning_stat_text(
        ui,
        draw_rect(
            TUNING_STAT_VALUE_X + 1.0,
            TUNING_TYPE_VALUE_Y,
            TUNING_TYPE_VALUE_WIDTH,
        ),
        stats.vehicle_type_label,
        TUNING_TYPE_FONT_SIZE,
    );
    draw_tuning_stat_text(
        ui,
        draw_rect(
            TUNING_STAT_VALUE_X + 6.0,
            TUNING_DEF_VALUE_Y - 2.0,
            TUNING_STAT_VALUE_WIDTH,
        ),
        &stats.defence.to_string(),
        TUNING_STAT_FONT_SIZE,
    );
    draw_tuning_stat_text(
        ui,
        draw_rect(
            TUNING_STAT_VALUE_X + 6.0,
            TUNING_MDEF_VALUE_Y - 2.0,
            TUNING_STAT_VALUE_WIDTH,
        ),
        &stats.magic_defence.to_string(),
        TUNING_STAT_FONT_SIZE,
    );
    draw_tuning_stat_text(
        ui,
        draw_rect(
            TUNING_STAT_VALUE_X + 6.0,
            TUNING_FUEL_VALUE_Y - 1.0,
            TUNING_STAT_VALUE_WIDTH,
        ),
        &stats.fuel_consumption.to_string(),
        TUNING_STAT_FONT_SIZE,
    );
    draw_tuning_stat_text(
        ui,
        draw_rect(
            TUNING_STAT_VALUE_X + 6.0,
            TUNING_SPEED_VALUE_Y - 1.0,
            TUNING_STAT_VALUE_WIDTH,
        ),
        &stats.speed.to_string(),
        TUNING_STAT_FONT_SIZE,
    );
    draw_tuning_stat_text(
        ui,
        draw_rect(
            TUNING_STAT_VALUE_X + 6.0,
            TUNING_ATK_VALUE_Y + 23.0,
            TUNING_STAT_VALUE_WIDTH,
        ),
        &stats.attack.to_string(),
        TUNING_STAT_FONT_SIZE,
    );
    draw_tuning_stat_text(
        ui,
        draw_rect(
            TUNING_STAT_VALUE_X + 6.0,
            TUNING_ASPD_VALUE_Y + 23.0,
            TUNING_STAT_VALUE_WIDTH,
        ),
        &stats.attack_speed.to_string(),
        TUNING_STAT_FONT_SIZE,
    );
}

fn drag_accepts_equipment(drag_source: &DragAndDropId) -> bool {
    matches!(
        drag_source,
        DragAndDropId::Inventory(ItemSlot::Inventory(InventoryPageType::Equipment, _))
            | DragAndDropId::Inventory(ItemSlot::Equipment(_))
            | DragAndDropId::Inventory(ItemSlot::Inventory(InventoryPageType::Materials, _))
    )
}

fn drag_accepts_equipment_or_bank(drag_source: &DragAndDropId) -> bool {
    drag_accepts_equipment(drag_source)
        || matches!(
            drag_source,
            DragAndDropId::Bank(_) | DragAndDropId::PersonalStoreSell(_)
        )
}

fn drag_accepts_consumables(drag_source: &DragAndDropId) -> bool {
    matches!(
        drag_source,
        DragAndDropId::Inventory(ItemSlot::Inventory(InventoryPageType::Consumables, _))
    )
}

fn drag_accepts_consumables_or_bank(drag_source: &DragAndDropId) -> bool {
    drag_accepts_consumables(drag_source)
        || matches!(
            drag_source,
            DragAndDropId::Bank(_) | DragAndDropId::PersonalStoreSell(_)
        )
}

fn drag_accepts_materials(drag_source: &DragAndDropId) -> bool {
    matches!(
        drag_source,
        DragAndDropId::Inventory(ItemSlot::Inventory(InventoryPageType::Materials, _))
            | DragAndDropId::Inventory(ItemSlot::Ammo(_))
    )
}

fn drag_accepts_materials_or_bank(drag_source: &DragAndDropId) -> bool {
    drag_accepts_materials(drag_source)
        || matches!(
            drag_source,
            DragAndDropId::Bank(_) | DragAndDropId::PersonalStoreSell(_)
        )
}

fn drag_accepts_vehicles(drag_source: &DragAndDropId) -> bool {
    matches!(
        drag_source,
        DragAndDropId::Inventory(ItemSlot::Inventory(InventoryPageType::Vehicles, _))
            | DragAndDropId::Inventory(ItemSlot::Vehicle(_))
    )
}

fn drag_accepts_vehicles_or_bank(drag_source: &DragAndDropId) -> bool {
    drag_accepts_vehicles(drag_source)
        || matches!(
            drag_source,
            DragAndDropId::Bank(_) | DragAndDropId::PersonalStoreSell(_)
        )
}

pub trait GetItem {
    fn get_item(&self, item_slot: ItemSlot) -> Option<Item>;
}

impl GetItem for (&Equipment, &Inventory) {
    fn get_item(&self, item_slot: ItemSlot) -> Option<Item> {
        let equipment = self.0;
        let inventory = self.1;

        match item_slot {
            ItemSlot::Inventory(_, _) => inventory.get_item(item_slot).cloned(),
            ItemSlot::Equipment(equipment_index) => equipment
                .get_equipment_item(equipment_index)
                .cloned()
                .map(Item::Equipment),
            ItemSlot::Ammo(ammo_index) => equipment
                .get_ammo_item(ammo_index)
                .cloned()
                .map(Item::Stackable),
            ItemSlot::Vehicle(vehicle_part_index) => equipment
                .get_vehicle_item(vehicle_part_index)
                .cloned()
                .map(Item::Equipment),
        }
    }
}

fn ui_add_inventory_slot(
    ui: &mut egui::Ui,
    inventory_slot: ItemSlot,
    pos: egui::Pos2,
    player: &PlayerQueryItem,
    player_tooltip_data: Option<&PlayerTooltipQueryItem>,
    game_data: &GameData,
    ui_resources: &UiResources,
    item_slot_map: &mut EnumMap<InventoryPageType, Vec<ItemSlot>>,
    ui_state_dnd: &mut UiStateDragAndDrop,
    player_command_events: &mut EventWriter<PlayerCommandEvent>,
    personal_store_events: &mut EventWriter<PersonalStoreEvent>,
) {
    let drag_accepts = match inventory_slot {
        ItemSlot::Inventory(page_type, _) => match page_type {
            InventoryPageType::Equipment => drag_accepts_equipment_or_bank,
            InventoryPageType::Consumables => drag_accepts_consumables_or_bank,
            InventoryPageType::Materials => drag_accepts_materials_or_bank,
            InventoryPageType::Vehicles => drag_accepts_vehicles_or_bank,
        },
        ItemSlot::Equipment(_) => drag_accepts_equipment,
        ItemSlot::Ammo(_) => drag_accepts_materials,
        ItemSlot::Vehicle(_) => drag_accepts_vehicles,
    };
    let item = (player.equipment, player.inventory).get_item(inventory_slot);
    let is_pending_sell = ui_state_dnd
        .pending_sell_item_slots
        .contains(&inventory_slot);

    let mut dropped_item = None;
    let response = ui
        .allocate_ui_at_rect(
            egui::Rect::from_min_size(ui.min_rect().min + pos.to_vec2(), egui::vec2(40.0, 40.0)),
            |ui| {
                egui::Widget::ui(
                    DragAndDropSlot::with_item(
                        DragAndDropId::Inventory(inventory_slot),
                        item.as_ref(),
                        Some(player.cooldowns),
                        game_data,
                        ui_resources,
                        drag_accepts,
                        &mut ui_state_dnd.dragged_item,
                        &mut dropped_item,
                        [40.0, 40.0],
                    )
                    .set_darkened(is_pending_sell),
                    ui,
                )
            },
        )
        .inner;

    let mut equip_equipment_inventory_slot = None;
    let mut equip_ammo_inventory_slot = None;
    let mut equip_vehicle_inventory_slot = None;
    let mut unequip_equipment_index = None;
    let mut unequip_ammo_index = None;
    let mut unequip_vehicle_part_index = None;
    let mut use_inventory_slot = None;
    let mut drop_inventory_slot = None;
    let mut swap_inventory_slots = None;

    if response.double_clicked() {
        match inventory_slot {
            ItemSlot::Inventory(InventoryPageType::Equipment, _) => {
                equip_equipment_inventory_slot = Some(inventory_slot);
            }
            ItemSlot::Inventory(InventoryPageType::Vehicles, _) => {
                equip_vehicle_inventory_slot = Some(inventory_slot);
            }
            ItemSlot::Inventory(InventoryPageType::Materials, _) => {
                equip_ammo_inventory_slot = Some(inventory_slot);
            }
            ItemSlot::Inventory(InventoryPageType::Consumables, _) => {
                use_inventory_slot = Some(inventory_slot);
            }
            ItemSlot::Equipment(equipment_index) => {
                unequip_equipment_index = Some(equipment_index);
            }
            ItemSlot::Ammo(ammo_index) => {
                unequip_ammo_index = Some(ammo_index);
            }
            ItemSlot::Vehicle(vehicle_part_index) => {
                unequip_vehicle_part_index = Some(vehicle_part_index);
            }
        }
    }

    if let Some(item) = item {
        let response = response.context_menu(|ui| {
            if matches!(
                inventory_slot,
                ItemSlot::Inventory(InventoryPageType::Equipment, _)
            ) && ui.button("Equip").clicked()
            {
                equip_equipment_inventory_slot = Some(inventory_slot);
            }

            if matches!(
                inventory_slot,
                    | ItemSlot::Inventory(InventoryPageType::Vehicles, _)
            ) && ui.button("Equip").clicked()
            {
                equip_vehicle_inventory_slot = Some(inventory_slot);
            }

            if matches!(
                inventory_slot,
                    | ItemSlot::Inventory(InventoryPageType::Materials, _)
            ) && ui.button("Equip").clicked()
            {
                equip_ammo_inventory_slot = Some(inventory_slot);
            }

            if let ItemSlot::Equipment(equipment_index) = inventory_slot {
                if ui.button("Unequip").clicked() {
                    unequip_equipment_index = Some(equipment_index);
                }
            }

            if matches!(
                inventory_slot,
                ItemSlot::Inventory(InventoryPageType::Consumables, _)
            ) && ui.button("Use").clicked()
            {
                use_inventory_slot = Some(inventory_slot);
            }

            if matches!(inventory_slot, ItemSlot::Inventory(_, _)) && ui.button("Drop").clicked() {
                drop_inventory_slot = Some(inventory_slot);
            }
        });

        response.on_hover_ui(|ui| {
            ui_add_item_tooltip(ui, game_data, player_tooltip_data, &item);
        });
    }

    if let Some(DragAndDropId::Inventory(dropped_inventory_slot)) = dropped_item {
        match inventory_slot {
            ItemSlot::Inventory(_, _) => match dropped_inventory_slot {
                ItemSlot::Inventory(_, _) => {
                    swap_inventory_slots = Some((inventory_slot, dropped_inventory_slot))
                }
                ItemSlot::Equipment(equipment_index) => {
                    unequip_equipment_index = Some(equipment_index);
                }
                ItemSlot::Ammo(ammo_index) => {
                    unequip_ammo_index = Some(ammo_index);
                }
                ItemSlot::Vehicle(vehicle_part_index) => {
                    unequip_vehicle_part_index = Some(vehicle_part_index);
                }
            },
            ItemSlot::Equipment(target_equipment_index) => {
                if matches!(
                    dropped_inventory_slot,
                    ItemSlot::Inventory(InventoryPageType::Equipment, _)
                ) {
                    equip_equipment_inventory_slot = Some(dropped_inventory_slot);
                } else if matches!(
                    dropped_inventory_slot,
                    ItemSlot::Inventory(InventoryPageType::Materials, _)
                ) {
                    // Check if the dropped item is a gem and the target has a socket
                    let is_gem = (player.equipment, player.inventory)
                        .get_item(dropped_inventory_slot)
                        .map_or(false, |item| {
                            item.get_item_type() == ItemType::Gem
                                && game_data
                                    .items
                                    .get_base_item(item.get_item_reference())
                                    .map_or(false, |item_data| item_data.class == ItemClass::Jewel)
                        });
                    let target_has_empty_socket = player
                        .equipment
                        .get_equipment_item(target_equipment_index)
                        .map_or(false, |eq| eq.has_socket && eq.gem <= 300);

                    if is_gem && target_has_empty_socket {
                        player_command_events.send(PlayerCommandEvent::InsertGem(
                            target_equipment_index,
                            dropped_inventory_slot,
                        ));
                    }
                }
            }
            ItemSlot::Ammo(_) => {
                if matches!(
                    dropped_inventory_slot,
                    ItemSlot::Inventory(InventoryPageType::Materials, _)
                ) {
                    equip_ammo_inventory_slot = Some(dropped_inventory_slot);
                }
            }
            ItemSlot::Vehicle(_) => {
                if matches!(
                    dropped_inventory_slot,
                    ItemSlot::Inventory(InventoryPageType::Vehicles, _)
                ) {
                    equip_vehicle_inventory_slot = Some(dropped_inventory_slot);
                }
            }
        }
    }

    if let Some(DragAndDropId::Bank(dropped_bank_slot_index)) = dropped_item {
        player_command_events.send(PlayerCommandEvent::BankWithdrawItem(
            dropped_bank_slot_index,
        ));
    }

    if let Some(DragAndDropId::PersonalStoreSell(slot_index)) = dropped_item {
        if matches!(inventory_slot, ItemSlot::Inventory(_, _)) {
            personal_store_events.send(PersonalStoreEvent::BuyItemBySlot { slot_index });
        }
    }

    if let Some(item_slot) = equip_equipment_inventory_slot {
        player_command_events.send(PlayerCommandEvent::EquipEquipment(item_slot));
    }

    if let Some(item_slot) = equip_ammo_inventory_slot {
        player_command_events.send(PlayerCommandEvent::EquipAmmo(item_slot));
    }

    if let Some(item_slot) = equip_vehicle_inventory_slot {
        player_command_events.send(PlayerCommandEvent::EquipVehicle(item_slot));
    }

    if let Some(ammo_index) = unequip_ammo_index {
        player_command_events.send(PlayerCommandEvent::UnequipAmmo(ammo_index));
    }

    if let Some(equipment_index) = unequip_equipment_index {
        player_command_events.send(PlayerCommandEvent::UnequipEquipment(equipment_index));
    }

    if let Some(vehicle_part_index) = unequip_vehicle_part_index {
        player_command_events.send(PlayerCommandEvent::UnequipVehicle(vehicle_part_index));
    }

    if let Some(use_inventory_slot) = use_inventory_slot {
        player_command_events.send(PlayerCommandEvent::UseItem(use_inventory_slot));
    }

    if let Some(drop_inventory_slot) = drop_inventory_slot {
        player_command_events.send(PlayerCommandEvent::DropItem(drop_inventory_slot));
    }

    if let Some((ItemSlot::Inventory(page_a, slot_a), ItemSlot::Inventory(page_b, slot_b))) =
        swap_inventory_slots
    {
        if page_a == page_b {
            let inventory_map = &mut item_slot_map[page_a];
            let source_index = inventory_map
                .iter()
                .position(|slot| slot == &ItemSlot::Inventory(page_a, slot_a));
            let destination_index = inventory_map
                .iter()
                .position(|slot| slot == &ItemSlot::Inventory(page_b, slot_b));
            if let (Some(source_index), Some(destination_index)) = (source_index, destination_index)
            {
                inventory_map.swap(source_index, destination_index);
            }
        }
    }
}

#[derive(WorldQuery)]
pub struct PlayerQuery<'w> {
    ability_values: &'w AbilityValues,
    equipment: &'w Equipment,
    inventory: &'w Inventory,
    cooldowns: &'w Cooldowns,
}

pub fn ui_inventory_system(
    mut egui_context: EguiContexts,
    mut ui_state_inventory: Local<UiStateInventory>,
    mut ui_state_dnd: ResMut<UiStateDragAndDrop>,
    mut ui_state_windows: ResMut<UiStateWindows>,
    mut ui_sound_events: EventWriter<UiSoundEvent>,
    query_player: Query<PlayerQuery, With<PlayerCharacter>>,
    query_player_tooltip: Query<PlayerTooltipQuery, With<PlayerCharacter>>,
    dialog_assets: Res<Assets<Dialog>>,
    game_data: Res<GameData>,
    ui_resources: Res<UiResources>,
    mut player_command_events: EventWriter<PlayerCommandEvent>,
    mut personal_store_events: EventWriter<PersonalStoreEvent>,
    mut number_input_dialog_events: EventWriter<NumberInputDialogEvent>,
) {
    let ui_state_inventory = &mut *ui_state_inventory;
    let dialog = if let Some(dialog) = ui_state_inventory
        .dialog_instance
        .get_mut(&dialog_assets, &ui_resources)
    {
        dialog
    } else {
        return;
    };
    let player = if let Ok(player) = query_player.get_single() {
        player
    } else {
        return;
    };
    let player_tooltip_data = query_player_tooltip.get_single().ok();

    let mut response_close_button = None;
    let mut response_minimise_button = None;
    let mut response_maximise_button = None;
    let mut response_drop_money_button = None;
    let is_equipment_tab = ui_state_inventory.current_equipment_tab == IID_TAB_EQUIP_AVATAR;
    let is_minimised = ui_state_inventory.minimised;

    egui::Window::new("Inventory")
        .frame(egui::Frame::none())
        .open(&mut ui_state_windows.inventory_open)
        .title_bar(false)
        .resizable(false)
        .default_width(dialog.width)
        .default_height(dialog.height)
        .show(egui_context.ctx_mut(), |ui| {
            dialog.draw(
                ui,
                DataBindings {
                    sound_events: Some(&mut ui_sound_events),
                    tabs: &mut [
                        (
                            IID_TABBEDPANE_EQUIP,
                            &mut ui_state_inventory.current_equipment_tab,
                        ),
                        (
                            IID_TABBEDPANE_INVEN_PAT,
                            &mut ui_state_inventory.current_vehicle_tab,
                        ),
                        (
                            IID_TABBEDPANE_INVEN_ITEM,
                            &mut ui_state_inventory.current_inventory_tab,
                        ),
                    ],
                    visible: &mut [
                        (IID_TABBEDPANE_INVEN_ITEM, is_equipment_tab),
                        (IID_TABBEDPANE_INVEN_PAT, !is_equipment_tab),
                        (IID_BTN_MINIMIZE, !is_minimised),
                        (IID_BTN_MAXIMIZE, is_minimised),
                    ],
                    response: &mut [
                        (IID_BTN_CLOSE, &mut response_close_button),
                        (IID_BTN_MINIMIZE, &mut response_minimise_button),
                        (IID_BTN_MAXIMIZE, &mut response_maximise_button),
                        (IID_BTN_MONEY, &mut response_drop_money_button),
                    ],
                    ..Default::default()
                },
                |ui, bindings| {
                    let mut current_page = InventoryPageType::Equipment;

                    match bindings.get_tab(IID_TABBEDPANE_EQUIP) {
                        Some(&mut IID_TAB_EQUIP_AVATAR) => {
                            if !ui_state_inventory.minimised {
                                for (item_slot, pos) in EQUIPMENT_GRID_SLOTS.iter() {
                                    ui_add_inventory_slot(
                                        ui,
                                        *item_slot,
                                        *pos + egui::vec2(-1.0, -1.0),
                                        &player,
                                        player_tooltip_data.as_ref(),
                                        &game_data,
                                        &ui_resources,
                                        &mut ui_state_inventory.item_slot_map,
                                        &mut ui_state_dnd,
                                        &mut player_command_events,
                                        &mut personal_store_events,
                                    );
                                }
                            }

                            match bindings.get_tab(IID_TABBEDPANE_INVEN_ITEM) {
                                Some(&mut IID_TAB_INVEN_EQUIP) => {
                                    current_page = InventoryPageType::Equipment;
                                }
                                Some(&mut IID_TAB_INVEN_USE) => {
                                    current_page = InventoryPageType::Consumables;
                                }
                                Some(&mut IID_TAB_INVEN_ETC) => {
                                    current_page = InventoryPageType::Materials;
                                }
                                _ => {}
                            }
                        }
                        Some(&mut IID_TAB_EQUIP_PAT) => {
                            if !ui_state_inventory.minimised {
                                for (item_slot, pos) in VEHICLE_GRID_SLOTS.iter() {
                                    ui_add_inventory_slot(
                                        ui,
                                        *item_slot,
                                        *pos + egui::vec2(-1.0, -3.0),
                                        &player,
                                        player_tooltip_data.as_ref(),
                                        &game_data,
                                        &ui_resources,
                                        &mut ui_state_inventory.item_slot_map,
                                        &mut ui_state_dnd,
                                        &mut player_command_events,
                                        &mut personal_store_events,
                                    );
                                }

                                let tuning_stats = VehicleTuningStats::from_sources(
                                    player.equipment,
                                    player.ability_values,
                                    &game_data,
                                );
                                draw_vehicle_tuning_stats(ui, &tuning_stats);
                            }

                            current_page = InventoryPageType::Vehicles;
                        }
                        _ => {}
                    }

                    let y_start = if ui_state_inventory.minimised {
                        83.0
                    } else {
                        283.0
                    };

                    for row in 0..6 {
                        for column in 0..5 {
                            let inventory_slot =
                                ui_state_inventory.item_slot_map[current_page][column + row * 5];

                            ui_add_inventory_slot(
                                ui,
                                inventory_slot,
                                egui::pos2(
                                    12.0 + column as f32 * 41.0,
                                    y_start + row as f32 * 41.0,
                                ),
                                &player,
                                player_tooltip_data.as_ref(),
                                &game_data,
                                &ui_resources,
                                &mut ui_state_inventory.item_slot_map,
                                &mut ui_state_dnd,
                                &mut player_command_events,
                                &mut personal_store_events,
                            );
                        }

                        ui.end_row();
                    }

                    // Allow dropping a personal shop item anywhere on the inventory panel
                    // without consuming pointer input needed by inventory hover/drag logic.
                    let panel_rect = egui::Rect::from_min_size(
                        ui.min_rect().min + egui::vec2(12.0, y_start),
                        egui::vec2(5.0 * 41.0, 6.0 * 41.0),
                    );
                    ui.ctx().input(|input| {
                        let pointer_over_panel = input
                            .pointer
                            .hover_pos()
                            .map_or(false, |pointer_pos| panel_rect.contains(pointer_pos));

                        if pointer_over_panel
                            && input.pointer.any_released()
                            && !input.pointer.button_down(egui::PointerButton::Primary)
                        {
                            if let Some(DragAndDropId::PersonalStoreSell(slot_index)) =
                                ui_state_dnd.dragged_item.as_ref()
                            {
                                personal_store_events.send(PersonalStoreEvent::BuyItemBySlot {
                                    slot_index: *slot_index,
                                });
                                ui_state_dnd.dragged_item = None;
                            }
                        }
                    });

                    ui.allocate_ui_at_rect(
                        ui.min_rect().translate(egui::vec2(
                            40.0,
                            dialog.height - 25.0 - if is_minimised { 200.0 } else { 0.0 },
                        )),
                        |ui| {
                            ui.horizontal_top(|ui| {
                                ui.add(egui::Label::new(format!("{}", player.inventory.money.0)))
                            })
                            .inner
                        },
                    );
                },
            );
        });

    if response_close_button.map_or(false, |r| r.clicked()) {
        ui_state_windows.inventory_open = false;
    }

    if response_minimise_button.map_or(false, |r| r.clicked()) {
        ui_state_inventory.minimised = true;

        if let Some(Widget::Pane(pane)) = dialog.get_widget_mut(IID_PANE_INVEN) {
            pane.y = 54.0;
        }
    }

    if response_maximise_button.map_or(false, |r| r.clicked()) {
        ui_state_inventory.minimised = false;

        if let Some(Widget::Pane(pane)) = dialog.get_widget_mut(IID_PANE_INVEN) {
            pane.y = 254.0;
        }
    }

    if response_drop_money_button.map_or(false, |r| r.clicked()) && player.inventory.money.0 > 0 {
        number_input_dialog_events.send(NumberInputDialogEvent::Show {
            max_value: Some(player.inventory.money.0 as usize),
            modal: false,
            ok: Some(Box::new(move |commands, amount| {
                commands.add(move |world: &mut World| {
                    if let Some(mut player_command_events) =
                        world.get_resource_mut::<Events<PlayerCommandEvent>>()
                    {
                        player_command_events.send(PlayerCommandEvent::DropMoney(amount));
                    }
                });
            })),
            cancel: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::VehicleTuningStats;
    use rose_data::VehicleType;
    use rose_game_common::components::{
        AbilityValues, AbilityValuesAdjust, DamageCategory, DamageType,
    };

    fn test_ability_values() -> AbilityValues {
        AbilityValues {
            is_driving: false,
            damage_category: DamageCategory::Character,
            level: 1,
            walk_speed: 200.0,
            run_speed: 300.0,
            vehicle_move_speed: 420.0,
            strength: 0,
            dexterity: 0,
            intelligence: 0,
            concentration: 0,
            charm: 0,
            sense: 0,
            max_health: 0,
            max_mana: 0,
            additional_health_recovery: 0,
            additional_mana_recovery: 0,
            attack_damage_type: DamageType::Physical,
            attack_power: 0,
            attack_speed: 0,
            passive_attack_speed: 0,
            attack_range: 0,
            hit: 0,
            defence: 0,
            resistance: 88,
            critical: 0,
            avoid: 0,
            vehicle_attack_power: 77,
            vehicle_attack_range: 0,
            vehicle_attack_speed: 123,
            vehicle_hit: 0,
            vehicle_defence: 66,
            vehicle_critical: 0,
            vehicle_avoid: 0,
            max_damage_sources: 0,
            drop_rate: 0,
            max_weight: 0,
            summon_owner_level: None,
            summon_skill_level: None,
            adjust: AbilityValuesAdjust {
                additional_damage_multiplier: 0.0,
                attack_speed: 7,
                attack_power: 5,
                avoid: 0,
                critical: 0,
                defence: 3,
                hit: 0,
                resistance: 2,
                max_health: 0,
                max_mana: 0,
                run_speed: 4.0,
            },
            npc_store_buy_rate: 0,
            npc_store_sell_rate: 0,
            save_mana: 0,
            passive_max_summons: 0,
        }
    }

    #[test]
    fn tuning_stats_full_cart() {
        let stats = VehicleTuningStats::from_resolved_parts(
            Some(VehicleType::Cart),
            Some(12),
            &test_ability_values(),
        );

        assert_eq!(stats.vehicle_type_label, "Cart");
        assert_eq!(stats.defence, 69);
        assert_eq!(stats.magic_defence, 90);
        assert_eq!(stats.fuel_consumption, 12);
        assert_eq!(stats.speed, 424);
        assert_eq!(stats.attack, 82);
        assert_eq!(stats.attack_speed, 130);
    }

    #[test]
    fn tuning_stats_full_castle_gear() {
        let stats = VehicleTuningStats::from_resolved_parts(
            Some(VehicleType::CastleGear),
            Some(8),
            &test_ability_values(),
        );

        assert_eq!(stats.vehicle_type_label, "Castle Gear");
        assert_eq!(stats.fuel_consumption, 8);
        assert_eq!(stats.speed, 424);
    }

    #[test]
    fn tuning_stats_no_body() {
        let stats = VehicleTuningStats::from_resolved_parts(None, Some(12), &test_ability_values());

        assert_eq!(stats.vehicle_type_label, "-");
        assert_eq!(stats.defence, 0);
        assert_eq!(stats.magic_defence, 0);
        assert_eq!(stats.fuel_consumption, 0);
        assert_eq!(stats.speed, 0);
        assert_eq!(stats.attack, 0);
        assert_eq!(stats.attack_speed, 0);
    }

    #[test]
    fn tuning_stats_no_engine() {
        let mut ability_values = test_ability_values();
        ability_values.vehicle_move_speed = 200.0;
        ability_values.adjust.run_speed = 0.0;

        let stats =
            VehicleTuningStats::from_resolved_parts(Some(VehicleType::Cart), None, &ability_values);

        assert_eq!(stats.vehicle_type_label, "Cart");
        assert_eq!(stats.fuel_consumption, 0);
        assert_eq!(stats.speed, 200);
    }

    #[test]
    fn tuning_stats_no_arms() {
        let mut ability_values = test_ability_values();
        ability_values.vehicle_attack_power = 0;
        ability_values.vehicle_attack_speed = 300;
        ability_values.adjust.attack_power = 0;
        ability_values.adjust.attack_speed = 0;

        let stats = VehicleTuningStats::from_resolved_parts(
            Some(VehicleType::Cart),
            Some(12),
            &ability_values,
        );

        assert_eq!(stats.attack, 0);
        assert_eq!(stats.attack_speed, 300);
    }
}
