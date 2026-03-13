use bevy::{
    ecs::query::WorldQuery,
    math::Vec3Swizzles,
    prelude::{Assets, EventReader, EventWriter, Local, Query, Res, ResMut, With},
};
use bevy_egui::{egui, EguiContexts};
use std::collections::HashMap;

use rose_data::{
    Item, ItemClass, ItemReference, ItemType, ProductData, ProductMaterial, SkillType,
};
use rose_game_common::components::{
    Inventory, InventoryPageType, ItemSlot, ManaPoints, Npc, SkillList, SkillSlot,
};
use rose_game_common::data::{
    disassemble_from_npc_price, manufacture_required_mp, manufacture_success_chance,
    upgrade_from_npc_price,
};
use rose_game_common::messages::client::ClientMessage;

use crate::{
    components::{PlayerCharacter, Position},
    events::{ChatboxEvent, CraftEvent},
    resources::{
        ClientEntityList, GameConnection, GameData, UiResources, UiSpriteSheetType, WorldRates,
    },
    ui::{
        tooltips::{PlayerTooltipQuery, PlayerTooltipQueryItem},
        ui_add_item_tooltip,
        widgets::{DataBindings, Dialog},
        DialogInstance, DragAndDropId, DragAndDropSlot, UiDisassembleSource, UiSoundEvent,
        UiStateDragAndDrop, UiStateWindows, UiUpgradeSource,
    },
};

// Widget IDs from original C++ dialogs
const IID_BTN_START: i32 = 10;
const IID_BTN_CLOSE: i32 = 11;
const IID_TEXT_COST: i32 = 5;
const IID_COMBOBOX_ITEM: i32 = 20;
const IID_COMBOBOX_CLASS: i32 = 25;

// Manufacture layout from original MakeDLG.cpp
const MANUFACTURE_PREVIEW_SLOT_X: f32 = 168.0;
const MANUFACTURE_PREVIEW_SLOT_Y: f32 = 98.0;
const MANUFACTURE_MATERIAL_SLOT_X: f32 = 168.0;
const MANUFACTURE_MATERIAL_SLOT_Y: f32 = 171.0;
const MANUFACTURE_MATERIAL_SLOT_STEP_Y: f32 = 46.0;
const MANUFACTURE_MATERIAL_NAME_X: f32 = 27.0;
const MANUFACTURE_MATERIAL_NAME_Y: f32 = 189.0;
const MANUFACTURE_MATERIAL_NAME_WIDTH: f32 = 88.0;
const MANUFACTURE_MATERIAL_COUNT_X: f32 = 115.0;
const MANUFACTURE_MATERIAL_COUNT_Y: f32 = 189.0;
const MANUFACTURE_MATERIAL_COUNT_WIDTH: f32 = 30.0;
const MANUFACTURE_MATERIAL_TEXT_STEP_Y: f32 = 47.0;
const MANUFACTURE_SUCCESS_X: f32 = 79.0;
const MANUFACTURE_SUCCESS_Y: f32 = 379.0;
const MANUFACTURE_SUCCESS_WIDTH: f32 = 34.0;
const MANUFACTURE_SUCCESS_HEIGHT: f32 = 15.0;
const MANUFACTURE_MP_X: f32 = 176.0;
const MANUFACTURE_MP_Y: f32 = 379.0;
const MANUFACTURE_MP_WIDTH: f32 = 34.0;
const MANUFACTURE_MP_HEIGHT: f32 = 15.0;

// Upgrade layout from original CUpgradeDlg.cpp
const UPGRADE_TARGET_SLOT_X: f32 = 169.0;
const UPGRADE_TARGET_SLOT_Y: f32 = 99.0;
const UPGRADE_MATERIAL_SLOT_X: f32 = 169.0;
const UPGRADE_MATERIAL_SLOT_Y: f32 = 172.0;
const UPGRADE_MATERIAL_SLOT_STEP_Y: f32 = 46.0;
const UPGRADE_TARGET_NAME_X: f32 = 40.0;
const UPGRADE_TARGET_NAME_Y: f32 = 122.0;
const UPGRADE_TARGET_NAME_WIDTH: f32 = 110.0;
const UPGRADE_TARGET_NAME_HEIGHT: f32 = 18.0;
const UPGRADE_MATERIAL_NAME_X: f32 = 27.0;
const UPGRADE_MATERIAL_NAME_Y: f32 = 189.0;
const UPGRADE_MATERIAL_NAME_WIDTH: f32 = 88.0;
const UPGRADE_MATERIAL_NAME_HEIGHT: f32 = 18.0;
const UPGRADE_MATERIAL_COUNT_X: f32 = 115.0;
const UPGRADE_MATERIAL_COUNT_Y: f32 = 189.0;
const UPGRADE_MATERIAL_COUNT_WIDTH: f32 = 30.0;
const UPGRADE_MATERIAL_COUNT_HEIGHT: f32 = 16.0;
const UPGRADE_MATERIAL_TEXT_STEP_Y: f32 = 47.0;
const UPGRADE_SUCCESS_X: f32 = 79.0;
const UPGRADE_SUCCESS_Y: f32 = 379.0;
const UPGRADE_SUCCESS_WIDTH: f32 = 34.0;
const UPGRADE_SUCCESS_HEIGHT: f32 = 15.0;
const UPGRADE_COST_X: f32 = 176.0;
const UPGRADE_COST_Y: f32 = 379.0;
const UPGRADE_COST_WIDTH: f32 = 34.0;
const UPGRADE_COST_HEIGHT: f32 = 15.0;
const DEBUG_UPGRADE_LAYOUT_OVERLAY: bool = false;

// Separation layout from original CSeparateDlg.cpp
const SEPARATION_INPUT_SLOT_X: f32 = 169.0;
const SEPARATION_INPUT_SLOT_Y: f32 = 99.0;
const SEPARATION_OUTPUT_SLOT_X: f32 = 169.0;
const SEPARATION_OUTPUT_SLOT_Y: f32 = 172.0;
const SEPARATION_OUTPUT_SLOT_STEP_Y: f32 = 46.0;
const SEPARATION_INPUT_NAME_X: f32 = 39.0;
const SEPARATION_INPUT_NAME_Y: f32 = 120.0;
const SEPARATION_INPUT_NAME_WIDTH: f32 = 113.0;
const SEPARATION_INPUT_NAME_HEIGHT: f32 = 18.0;
const SEPARATION_OUTPUT_NAME_X: f32 = 29.0;
const SEPARATION_OUTPUT_NAME_Y: f32 = 187.0;
const SEPARATION_OUTPUT_NAME_WIDTH: f32 = 85.0;
const SEPARATION_OUTPUT_NAME_HEIGHT: f32 = 18.0;
const SEPARATION_OUTPUT_COUNT_Y: f32 = 187.0;
const SEPARATION_OUTPUT_COUNT_HEIGHT: f32 = 18.0;
const SEPARATION_OUTPUT_COUNT_GAP_LEFT: f32 = 1.0;
const SEPARATION_OUTPUT_COUNT_GAP_RIGHT: f32 = 3.0;
const SEPARATION_OUTPUT_COUNT_TARGET_WIDTH: f32 = 42.0;
const SEPARATION_OUTPUT_COUNT_MIN_WIDTH: f32 = 34.0;
const SEPARATION_OUTPUT_COUNT_MAX_WIDTH: f32 = 44.0;
const SEPARATION_OUTPUT_COUNT_NUDGE_X: f32 = 0.0;
const SEPARATION_OUTPUT_COUNT_BASELINE_NUDGE_Y: f32 = 0.5;
const SEPARATION_OUTPUT_COUNT_FONT_SIZES: [f32; 3] = [10.0, 9.0, 8.0];
const SEPARATION_OUTPUT_TEXT_STEP_Y: f32 = 46.0;
const SEPARATION_MP_X: f32 = 176.0;
const SEPARATION_MP_Y: f32 = 379.0;
const SEPARATION_MP_WIDTH: f32 = 34.0;
const SEPARATION_MP_HEIGHT: f32 = 15.0;

/// State for the manufacture crafting window
pub struct UiCraftManufactureState {
    dialog_instance: DialogInstance,
    /// The skill slot used to open this window (needed for sending the craft message)
    craft_skill_item_make_number: u32,
    /// List of (item_type, item_number, name) that can be crafted with this skill
    craftable_items: Vec<(ItemType, usize, String)>,
    /// Distinct craft classes (mapped from item type)
    craft_classes: Vec<ItemType>,
    /// Indices into craftable_items for current selected class
    filtered_craftable_indices: Vec<usize>,
    /// Selected class index in craft_classes
    selected_class_index: i32,
    /// Selected item index in filtered_craftable_indices
    selected_item_index: i32,
    /// Material slots selected from inventory
    material_slots: [Option<ItemSlot>; 4],
}

impl Default for UiCraftManufactureState {
    fn default() -> Self {
        Self {
            dialog_instance: DialogInstance::new("DLGMAKE.XML"),
            craft_skill_item_make_number: 0,
            craftable_items: Vec::new(),
            craft_classes: Vec::new(),
            filtered_craftable_indices: Vec::new(),
            selected_class_index: 0,
            selected_item_index: 0,
            material_slots: [None; 4],
        }
    }
}

/// State for the upgrade window
pub struct UiCraftUpgradeState {
    dialog_instance: DialogInstance,
    source: Option<UiUpgradeSource>,
    /// The equipment item slot to upgrade
    item_slot: Option<ItemSlot>,
    /// Ingredient item slots
    ingredient_slots: [Option<ItemSlot>; 3],
}

impl Default for UiCraftUpgradeState {
    fn default() -> Self {
        Self {
            dialog_instance: DialogInstance::new("DLGUPGRADE.XML"),
            source: None,
            item_slot: None,
            ingredient_slots: [None; 3],
        }
    }
}

/// State for the disassemble window
pub struct UiCraftDisassembleState {
    dialog_instance: DialogInstance,
    source: Option<UiDisassembleSource>,
    /// The item to disassemble
    item_slot: Option<ItemSlot>,
}

impl Default for UiCraftDisassembleState {
    fn default() -> Self {
        Self {
            dialog_instance: DialogInstance::new("DLGSEPARATE.XML"),
            source: None,
            item_slot: None,
        }
    }
}

pub struct UiCraftState {
    manufacture: UiCraftManufactureState,
    upgrade: UiCraftUpgradeState,
    disassemble: UiCraftDisassembleState,
    /// Track the last manufacture skill number to detect skill changes
    last_manufacture_skill: u32,
    /// Track the last manufacture skill level to detect level changes
    last_manufacture_skill_level: u32,
}

impl Default for UiCraftState {
    fn default() -> Self {
        Self {
            manufacture: UiCraftManufactureState::default(),
            upgrade: UiCraftUpgradeState::default(),
            disassemble: UiCraftDisassembleState::default(),
            last_manufacture_skill: 0,
            last_manufacture_skill_level: 0,
        }
    }
}

#[derive(WorldQuery)]
pub struct PlayerQuery<'w> {
    inventory: &'w Inventory,
    mana_points: &'w ManaPoints,
    position: &'w Position,
    skill_list: &'w SkillList,
}

/// Find the crafting skill's SkillSlot and item_make_number for CreateWindow skills
fn find_crafting_skill(
    skill_list: &SkillList,
    game_data: &GameData,
    make_number_range: std::ops::RangeInclusive<u32>,
) -> Option<(SkillSlot, u32)> {
    for page in &skill_list.pages {
        for (index, slot) in page.skills.iter().enumerate() {
            if let Some(skill_id) = slot {
                if let Some(skill_data) = game_data.skills.get_skill(*skill_id) {
                    if skill_data.skill_type == SkillType::CreateWindow
                        && make_number_range.contains(&skill_data.item_make_number)
                    {
                        return Some((
                            SkillSlot(page.page_type, index),
                            skill_data.item_make_number,
                        ));
                    }
                }
            }
        }
    }
    None
}

fn validate_crafting_skill_slot(
    skill_list: &SkillList,
    game_data: &GameData,
    skill_slot: Option<SkillSlot>,
    make_number_range: std::ops::RangeInclusive<u32>,
    expected_make_number: Option<u32>,
) -> Option<(SkillSlot, u32)> {
    let skill_slot = skill_slot?;
    let skill_data = skill_list
        .get_skill(skill_slot)
        .and_then(|skill_id| game_data.skills.get_skill(skill_id))?;

    if skill_data.skill_type != SkillType::CreateWindow {
        return None;
    }

    if !make_number_range.contains(&skill_data.item_make_number) {
        return None;
    }

    if let Some(expected_make_number) = expected_make_number {
        if skill_data.item_make_number != expected_make_number {
            return None;
        }
    }

    Some((skill_slot, skill_data.item_make_number))
}

/// Build the list of items that can be manufactured with the given craft skill type
fn build_craftable_item_list(
    game_data: &GameData,
    craft_skill_type: u32,
    craft_skill_level: u32,
) -> Vec<(ItemType, usize, String)> {
    let mut items = Vec::new();

    let item_types = [
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
        ItemType::Vehicle,
    ];

    for &item_type in &item_types {
        for item_ref in game_data.items.iter_items(item_type) {
            if let Some(base_item) = game_data.items.get_base_item(item_ref) {
                if base_item.craft_skill_type == craft_skill_type
                    && base_item.craft_material > 0
                    && base_item.craft_skill_level <= craft_skill_level
                {
                    items.push((item_type, item_ref.item_number, base_item.name.to_string()));
                }
            }
        }
    }

    items.sort_by(|a, b| a.2.cmp(&b.2));
    items
}

fn craft_class_name(item_type: ItemType) -> &'static str {
    match item_type {
        ItemType::Face => "Face",
        ItemType::Head => "Head",
        ItemType::Body => "Body",
        ItemType::Hands => "Hands",
        ItemType::Feet => "Feet",
        ItemType::Back => "Back",
        ItemType::Jewellery => "Jewellery",
        ItemType::Weapon => "Weapon",
        ItemType::SubWeapon => "Sub Weapon",
        ItemType::Consumable => "Consumable",
        ItemType::Gem => "Gem",
        ItemType::Material => "Material",
        ItemType::Vehicle => "Vehicle",
        _ => "Other",
    }
}

fn build_craft_class_list(craftable_items: &[(ItemType, usize, String)]) -> Vec<ItemType> {
    let mut classes = Vec::new();
    for (item_type, _, _) in craftable_items.iter() {
        if !classes.iter().any(|class| class == item_type) {
            classes.push(*item_type);
        }
    }
    classes.sort_by_key(|item_type| craft_class_name(*item_type));
    classes
}

fn refresh_manufacture_filtered_items(state: &mut UiCraftManufactureState) {
    state.filtered_craftable_indices.clear();

    if state.craft_classes.is_empty() {
        state.selected_class_index = 0;
        state.selected_item_index = 0;
        return;
    }

    state.selected_class_index = state
        .selected_class_index
        .clamp(0, state.craft_classes.len().saturating_sub(1) as i32);

    let selected_class = state.craft_classes[state.selected_class_index as usize];
    for (index, (item_type, _, _)) in state.craftable_items.iter().enumerate() {
        if *item_type == selected_class {
            state.filtered_craftable_indices.push(index);
        }
    }

    if state.filtered_craftable_indices.is_empty() {
        state.selected_item_index = 0;
    } else {
        state.selected_item_index = state.selected_item_index.clamp(
            0,
            state.filtered_craftable_indices.len().saturating_sub(1) as i32,
        );
    }
}

fn selected_manufacture_item(
    state: &UiCraftManufactureState,
) -> Option<(usize, &(ItemType, usize, String))> {
    let filtered_index = state
        .filtered_craftable_indices
        .get(state.selected_item_index.max(0) as usize)?;
    state
        .craftable_items
        .get(*filtered_index)
        .map(|item| (*filtered_index, item))
}

fn get_product_with_fallback<'a>(
    game_data: &'a GameData,
    product_id: u32,
) -> Option<&'a ProductData> {
    game_data
        .products
        .get_product(product_id)
        .or_else(|| {
            product_id
                .checked_sub(1)
                .and_then(|id| game_data.products.get_product(id))
        })
        .or_else(|| {
            product_id
                .checked_add(1)
                .and_then(|id| game_data.products.get_product(id))
        })
}

#[derive(Copy, Clone)]
enum ManufactureRequiredMaterialKind {
    ExactItem(ItemReference),
    ItemClass(ItemClass),
    Unknown,
}

struct ManufactureRequiredMaterial {
    name: String,
    quantity: u32,
    kind: ManufactureRequiredMaterialKind,
}

struct UpgradeRequirementSet {
    resolved: bool,
    requirements: [Option<ManufactureRequiredMaterial>; 3],
}

struct DisassemblePreviewRow {
    name: String,
    range_min: u32,
    range_max: u32,
    icon_sprite: Option<crate::resources::UiSprite>,
}

fn resolve_disassemble_row_item_reference(
    game_data: &GameData,
    target_item_ref: ItemReference,
    target_item_quality: u32,
    raw_material_type: u32,
    slot_idx: usize,
    material: &ProductMaterial,
) -> Option<ItemReference> {
    if game_data.items.get_base_item(material.item).is_some() {
        return Some(material.item);
    }

    if slot_idx != 0 {
        return None;
    }

    if raw_material_type >= 1000 {
        return game_data
            .data_decoder
            .decode_item_base1000(raw_material_type as usize);
    }

    if raw_material_type > 0 {
        // Original decomposition-style mapping for class-coded raw materials.
        let quality = target_item_quality as i32;
        let tier = ((quality - 20) / 12).clamp(1, 10);
        let derived_item_number = (raw_material_type as i32 - 421) * 10 + tier;
        if derived_item_number > 0 {
            let derived_item_ref =
                ItemReference::new(ItemType::Material, derived_item_number as usize);
            if game_data.items.get_base_item(derived_item_ref).is_some() {
                return Some(derived_item_ref);
            }
        }

        log::debug!(
            "Failed to derive disassemble row-0 preview item: target={:?} #{}, quality={}, raw_material_type={}, derived_item_number={}",
            target_item_ref.item_type,
            target_item_ref.item_number,
            target_item_quality,
            raw_material_type,
            derived_item_number
        );
    }

    None
}

fn build_disassemble_preview_row(
    game_data: &GameData,
    ui_resources: &UiResources,
    target_item_ref: ItemReference,
    target_item_quality: u32,
    raw_material_type: u32,
    slot_idx: usize,
    material: &ProductMaterial,
) -> DisassemblePreviewRow {
    let resolved =
        resolve_manufacture_required_material(game_data, material.item, material.quantity);
    let mut name = resolved.name;
    let mut icon_sprite = None;

    if let Some(preview_item_ref) = resolve_disassemble_row_item_reference(
        game_data,
        target_item_ref,
        target_item_quality,
        raw_material_type,
        slot_idx,
        material,
    ) {
        if let Some(preview_item_data) = game_data.items.get_base_item(preview_item_ref) {
            name = preview_item_data.name.to_string();
            icon_sprite = ui_resources.get_sprite_by_index(
                UiSpriteSheetType::Item,
                preview_item_data.icon_index as usize,
            );
        }
    }

    let range_min = ((material.quantity * 50) / 100).max(1);
    let range_max = ((material.quantity * 75) / 100).max(1);
    DisassemblePreviewRow {
        name,
        range_min,
        range_max,
        icon_sprite,
    }
}

fn format_disassemble_range_text(range_min: u32, range_max: u32) -> String {
    format!("{}-{}", range_min, range_max)
}

fn get_separation_output_count_rect(ui: &egui::Ui, row_index: f32) -> egui::Rect {
    let min_left =
        SEPARATION_OUTPUT_NAME_X + SEPARATION_OUTPUT_NAME_WIDTH + SEPARATION_OUTPUT_COUNT_GAP_LEFT;
    let max_right = SEPARATION_OUTPUT_SLOT_X - SEPARATION_OUTPUT_COUNT_GAP_RIGHT;
    let usable_width = (max_right - min_left).max(1.0);
    let count_width = SEPARATION_OUTPUT_COUNT_TARGET_WIDTH
        .max(SEPARATION_OUTPUT_COUNT_MIN_WIDTH)
        .min(usable_width.min(SEPARATION_OUTPUT_COUNT_MAX_WIDTH));
    let count_left = max_right - count_width;

    egui::Rect::from_min_size(
        ui.min_rect().min
            + egui::vec2(
                count_left,
                SEPARATION_OUTPUT_COUNT_Y + row_index * SEPARATION_OUTPUT_TEXT_STEP_Y,
            ),
        egui::vec2(count_width, SEPARATION_OUTPUT_COUNT_HEIGHT),
    )
}

fn pick_separation_output_count_font(
    ui: &egui::Ui,
    count_rect: egui::Rect,
    text: &str,
) -> egui::FontId {
    for font_size in SEPARATION_OUTPUT_COUNT_FONT_SIZES {
        let font = egui::FontId::proportional(font_size);
        let text_width = ui.fonts(|fonts| {
            fonts
                .layout_no_wrap(text.to_string(), font.clone(), egui::Color32::WHITE)
                .rect
                .width()
        });
        if text_width <= count_rect.width() {
            return font;
        }
    }

    egui::FontId::proportional(*SEPARATION_OUTPUT_COUNT_FONT_SIZES.last().unwrap_or(&8.0))
}

fn resolve_manufacture_required_material(
    game_data: &GameData,
    required_item: ItemReference,
    quantity: u32,
) -> ManufactureRequiredMaterial {
    if let Some(base_item) = game_data.items.get_base_item(required_item) {
        return ManufactureRequiredMaterial {
            name: base_item.name.to_string(),
            quantity,
            kind: ManufactureRequiredMaterialKind::ExactItem(required_item),
        };
    }

    if let Some(item_class) = game_data
        .data_decoder
        .decode_item_class(required_item.item_number)
    {
        let class_name = game_data.string_database.get_item_class(item_class);
        let class_name = if class_name.is_empty() {
            format!("Class #{}", required_item.item_number)
        } else {
            class_name.to_string()
        };
        return ManufactureRequiredMaterial {
            name: class_name,
            quantity,
            kind: ManufactureRequiredMaterialKind::ItemClass(item_class),
        };
    }

    log::debug!(
        "Unresolved manufacture material requirement {:?} #{}",
        required_item.item_type,
        required_item.item_number
    );
    ManufactureRequiredMaterial {
        name: format!(
            "Unknown {:?} #{}",
            required_item.item_type, required_item.item_number
        ),
        quantity,
        kind: ManufactureRequiredMaterialKind::Unknown,
    }
}

fn validate_manufacture_material_drop(
    game_data: &GameData,
    player: &PlayerQueryItem,
    dropped_inventory_slot: ItemSlot,
    requirement: Option<&ManufactureRequiredMaterial>,
) -> Result<(), &'static str> {
    let Some(requirement) = requirement else {
        return Err("No material is required for this slot.");
    };

    let Some(item) = player.inventory.get_item(dropped_inventory_slot) else {
        return Err("Invalid inventory item.");
    };

    let Item::Stackable(stackable) = item else {
        return Err("Wrong material for this slot.");
    };

    let matches_requirement = match requirement.kind {
        ManufactureRequiredMaterialKind::ExactItem(required_item) => {
            stackable.item == required_item
        }
        ManufactureRequiredMaterialKind::ItemClass(required_class) => game_data
            .items
            .get_base_item(stackable.item)
            .map_or(false, |item_data| item_data.class == required_class),
        ManufactureRequiredMaterialKind::Unknown => false,
    };
    if !matches_requirement {
        return Err("Wrong material for this slot.");
    }

    if stackable.quantity < requirement.quantity {
        return Err("Not enough quantity for this material.");
    }

    Ok(())
}

fn build_upgrade_target_name(
    game_data: &GameData,
    player: &PlayerQueryItem,
    target_item_slot: Option<ItemSlot>,
) -> Option<String> {
    let target_item = target_item_slot.and_then(|slot| player.inventory.get_item(slot))?;
    let base_item = game_data
        .items
        .get_base_item(target_item.get_item_reference())?;

    let grade_text = target_item
        .as_equipment()
        .map(|equipment| format!(" (+{})", equipment.grade))
        .unwrap_or_default();

    Some(format!("{}{}", base_item.name, grade_text))
}

fn resolve_upgrade_product_row_id(target_item: ItemReference, grade: u8) -> Option<u32> {
    if !target_item.item_type.is_equipment_item() {
        return None;
    }

    let base_row = match target_item.item_type {
        ItemType::Weapon => 1u32,
        _ => 11u32,
    };
    Some(base_row + grade as u32)
}

fn build_upgrade_requirements(
    game_data: &GameData,
    player: &PlayerQueryItem,
    target_item_slot: Option<ItemSlot>,
) -> UpgradeRequirementSet {
    let mut requirements = [None, None, None];

    let Some(target_item) = target_item_slot.and_then(|slot| player.inventory.get_item(slot))
    else {
        return UpgradeRequirementSet {
            resolved: false,
            requirements,
        };
    };

    let Some(equipment) = target_item.as_equipment() else {
        return UpgradeRequirementSet {
            resolved: false,
            requirements,
        };
    };

    let target_item_ref = target_item.get_item_reference();
    let Some(product_row_id) = resolve_upgrade_product_row_id(target_item_ref, equipment.grade)
    else {
        log::debug!(
            "Unable to resolve upgrade product row for requirements: {:?} #{} grade {}",
            target_item_ref.item_type,
            target_item_ref.item_number,
            equipment.grade
        );
        return UpgradeRequirementSet {
            resolved: false,
            requirements,
        };
    };

    let Some(product) = game_data.products.get_product(product_row_id) else {
        log::debug!(
            "Missing upgrade product row {} for requirements, target {:?} #{} grade {}",
            product_row_id,
            target_item_ref.item_type,
            target_item_ref.item_number,
            equipment.grade
        );
        return UpgradeRequirementSet {
            resolved: false,
            requirements,
        };
    };

    if let Some(material) = product.materials.get(0) {
        let row0_requirement = if product.raw_material_type > 0 {
            if let Some(item_class) = game_data
                .data_decoder
                .decode_item_class(product.raw_material_type as usize)
            {
                let class_name = game_data.string_database.get_item_class(item_class);
                let class_name = if class_name.is_empty() {
                    format!("Class #{}", product.raw_material_type)
                } else {
                    class_name.to_string()
                };
                ManufactureRequiredMaterial {
                    name: class_name,
                    quantity: material.quantity,
                    kind: ManufactureRequiredMaterialKind::ItemClass(item_class),
                }
            } else {
                resolve_manufacture_required_material(game_data, material.item, material.quantity)
            }
        } else {
            resolve_manufacture_required_material(game_data, material.item, material.quantity)
        };

        requirements[0] = Some(row0_requirement);
    }

    if let Some(material) = product.materials.get(1) {
        requirements[1] = Some(resolve_manufacture_required_material(
            game_data,
            material.item,
            material.quantity,
        ));
    }

    if let Some(material) = product.materials.get(2) {
        requirements[2] = Some(resolve_manufacture_required_material(
            game_data,
            material.item,
            material.quantity,
        ));
    }

    UpgradeRequirementSet {
        resolved: true,
        requirements,
    }
}

fn build_upgrade_required_quantities(
    upgrade_requirements: &UpgradeRequirementSet,
) -> [Option<u32>; 3] {
    [
        upgrade_requirements.requirements[0]
            .as_ref()
            .map(|requirement| requirement.quantity),
        upgrade_requirements.requirements[1]
            .as_ref()
            .map(|requirement| requirement.quantity),
        upgrade_requirements.requirements[2]
            .as_ref()
            .map(|requirement| requirement.quantity),
    ]
}

fn build_upgrade_material_rows(
    game_data: &GameData,
    upgrade_requirements: &UpgradeRequirementSet,
    player: &PlayerQueryItem,
    ingredient_slots: &[Option<ItemSlot>; 3],
) -> [Option<(String, u32)>; 3] {
    let mut rows: [Option<(String, u32)>; 3] = [None, None, None];

    for (row_index, requirement) in upgrade_requirements.requirements.iter().enumerate() {
        if let Some(requirement) = requirement {
            rows[row_index] = Some((requirement.name.clone(), requirement.quantity));
        }
    }

    // Fallback to currently slotted ingredient names/counts for unresolved rows.
    for (row_index, ingredient_slot) in ingredient_slots.iter().enumerate() {
        if rows[row_index].is_some() {
            continue;
        }

        if let Some(item) = ingredient_slot.and_then(|slot| player.inventory.get_item(slot)) {
            if let Some(base_item) = game_data.items.get_base_item(item.get_item_reference()) {
                rows[row_index] = Some((base_item.name.to_string(), item.get_quantity()));
            }
        }
    }

    rows
}

fn draw_upgrade_centered_text(
    ui: &egui::Ui,
    rect: egui::Rect,
    text: &str,
    font: egui::FontId,
    color: egui::Color32,
) {
    ui.painter().text(
        rect.center_top(),
        egui::Align2::CENTER_TOP,
        text,
        font,
        color,
    );
}

fn draw_upgrade_right_aligned_text(
    ui: &egui::Ui,
    rect: egui::Rect,
    text: &str,
    font: egui::FontId,
    color: egui::Color32,
) {
    ui.painter()
        .text(rect.right_top(), egui::Align2::RIGHT_TOP, text, font, color);
}

fn drag_accepts_from_inventory(drag_source: &DragAndDropId) -> bool {
    matches!(
        drag_source,
        DragAndDropId::Inventory(ItemSlot::Inventory(_, _))
    )
}

fn ui_add_craft_item_slot(
    ui: &mut egui::Ui,
    dnd_id: DragAndDropId,
    pos: egui::Pos2,
    item_slot: Option<ItemSlot>,
    player: &PlayerQueryItem,
    player_tooltip_data: Option<&PlayerTooltipQueryItem>,
    game_data: &GameData,
    ui_resources: &UiResources,
    ui_state_dnd: &mut UiStateDragAndDrop,
    quantity_cap: Option<u32>,
) -> Option<DragAndDropId> {
    let item = item_slot.and_then(|slot| player.inventory.get_item(slot));
    let mut dropped_item = None;
    let display_quantity_override = match (item, quantity_cap) {
        (Some(Item::Stackable(stackable_item)), Some(cap)) => {
            Some(stackable_item.quantity.min(cap) as usize)
        }
        _ => None,
    };

    let response = ui
        .allocate_ui_at_rect(
            egui::Rect::from_min_size(ui.min_rect().min + pos.to_vec2(), egui::vec2(40.0, 40.0)),
            |ui| {
                let mut widget = DragAndDropSlot::with_item(
                    dnd_id,
                    item,
                    None,
                    game_data,
                    ui_resources,
                    drag_accepts_from_inventory,
                    &mut ui_state_dnd.dragged_item,
                    &mut dropped_item,
                    [40.0, 40.0],
                );
                if display_quantity_override.is_some() {
                    widget = widget.set_quantity(display_quantity_override);
                }
                egui::Widget::ui(widget, ui)
            },
        )
        .inner;

    if let Some(item) = item {
        response.on_hover_ui(|ui| {
            ui_add_item_tooltip(ui, game_data, player_tooltip_data, item);
        });
    }

    dropped_item
}

pub fn ui_craft_system(
    mut egui_context: EguiContexts,
    mut ui_state: Local<UiCraftState>,
    mut ui_state_dnd: ResMut<UiStateDragAndDrop>,
    mut ui_state_windows: ResMut<UiStateWindows>,
    mut ui_sound_events: EventWriter<UiSoundEvent>,
    ui_resources: Res<UiResources>,
    dialog_assets: Res<Assets<Dialog>>,
    query_player: Query<PlayerQuery, With<PlayerCharacter>>,
    query_npc: Query<&Position, With<Npc>>,
    query_player_tooltip: Query<PlayerTooltipQuery, With<PlayerCharacter>>,
    client_entity_list: Res<ClientEntityList>,
    game_connection: Option<Res<GameConnection>>,
    game_data: Res<GameData>,
    world_rates: Option<Res<WorldRates>>,
    mut chatbox_events: EventWriter<ChatboxEvent>,
    mut craft_events: EventReader<CraftEvent>,
) {
    for event in craft_events.iter() {
        match *event {
            CraftEvent::UpgradeCompleted => {
                ui_state.upgrade.ingredient_slots = [None; 3];
            }
            CraftEvent::OpenNpcDisassemble { client_entity_id } => {
                if client_entity_list
                    .get(client_entity_id)
                    .and_then(|entity| query_npc.get(entity).ok())
                    .is_some()
                {
                    ui_state_windows.inventory_open = true;
                    ui_state_windows.craft_disassemble_open = true;
                    ui_state_windows.craft_disassemble_source =
                        Some(UiDisassembleSource::Npc(client_entity_id));
                    ui_state.disassemble = UiCraftDisassembleState::default();
                }
            }
            CraftEvent::OpenNpcUpgrade { client_entity_id } => {
                if client_entity_list
                    .get(client_entity_id)
                    .and_then(|entity| query_npc.get(entity).ok())
                    .is_some()
                {
                    ui_state_windows.inventory_open = true;
                    ui_state_windows.craft_upgrade_open = true;
                    ui_state_windows.craft_upgrade_source =
                        Some(UiUpgradeSource::Npc(client_entity_id));
                    ui_state.upgrade = UiCraftUpgradeState::default();
                }
            }
        }
    }

    let player = if let Ok(player) = query_player.get_single() {
        player
    } else {
        return;
    };
    let player_tooltip_data = query_player_tooltip.get_single().ok();

    // =================== MANUFACTURE WINDOW ===================
    if ui_state_windows.craft_manufacture_open {
        let mut craft_skill = validate_crafting_skill_slot(
            &player.skill_list,
            &game_data,
            ui_state_windows.craft_manufacture_skill_slot,
            11..=39,
            ui_state_windows.craft_manufacture_make_number,
        );
        if craft_skill.is_none() {
            craft_skill = find_crafting_skill(&player.skill_list, &game_data, 11..=39);
            if let Some((skill_slot, make_number)) = craft_skill {
                log::warn!(
                    "Manufacture craft context missing/invalid, falling back to first matching skill."
                );
                ui_state_windows.craft_manufacture_skill_slot = Some(skill_slot);
                ui_state_windows.craft_manufacture_make_number = Some(make_number);
            }
        }

        if let Some((skill_slot, make_number)) = craft_skill {
            let ui_state = &mut *ui_state;
            let craft_skill_data = player
                .skill_list
                .get_skill(skill_slot)
                .and_then(|skill_id| game_data.skills.get_skill(skill_id));
            let craft_skill_level = craft_skill_data.map_or(0, |skill_data| skill_data.level);
            let required_mp_display = craft_skill_data.map_or(0, manufacture_required_mp);
            let world_craft_rate = world_rates.as_ref().map_or(100, |rates| rates.craft_rate);

            // Rebuild item list if skill or skill level changed.
            if ui_state.last_manufacture_skill != make_number
                || ui_state.last_manufacture_skill_level != craft_skill_level
            {
                ui_state.manufacture.craft_skill_item_make_number = make_number;
                ui_state.manufacture.craftable_items =
                    build_craftable_item_list(&game_data, make_number, craft_skill_level);
                ui_state.manufacture.craft_classes =
                    build_craft_class_list(&ui_state.manufacture.craftable_items);
                ui_state.manufacture.selected_class_index = 0;
                ui_state.manufacture.selected_item_index = 0;
                refresh_manufacture_filtered_items(&mut ui_state.manufacture);
                ui_state.manufacture.material_slots = [None; 4];
                ui_state.last_manufacture_skill = make_number;
                ui_state.last_manufacture_skill_level = craft_skill_level;
            }

            refresh_manufacture_filtered_items(&mut ui_state.manufacture);
            let selected_item_before =
                selected_manufacture_item(&ui_state.manufacture).map(|(index, _)| index);
            let mut selected_class_index = ui_state.manufacture.selected_class_index;
            let mut selected_item_index = ui_state.manufacture.selected_item_index;

            let class_names: Vec<String> = ui_state
                .manufacture
                .craft_classes
                .iter()
                .map(|item_type| craft_class_name(*item_type).to_string())
                .collect();
            let item_names: Vec<String> = ui_state
                .manufacture
                .filtered_craftable_indices
                .iter()
                .filter_map(|index| {
                    ui_state
                        .manufacture
                        .craftable_items
                        .get(*index)
                        .map(|(_, _, name)| name.clone())
                })
                .collect();
            let mut selected_item_preview_sprite = None;
            let mut selected_item_success_rate = 0;
            let mut required_materials: Vec<ManufactureRequiredMaterial> = Vec::new();

            if let Some((_, (item_type, item_number, _name))) =
                selected_manufacture_item(&ui_state.manufacture)
            {
                let item_ref = ItemReference::new(*item_type, *item_number);

                if let Some(base_item) = game_data.items.get_base_item(item_ref) {
                    selected_item_success_rate = manufacture_success_chance(
                        craft_skill_level,
                        base_item.craft_skill_level,
                        world_craft_rate,
                    );
                    selected_item_preview_sprite = ui_resources.get_sprite_by_index(
                        UiSpriteSheetType::Item,
                        base_item.icon_index as usize,
                    );

                    if let Some(product) =
                        get_product_with_fallback(&game_data, base_item.craft_material)
                    {
                        for material in product.materials.iter().take(4) {
                            required_materials.push(resolve_manufacture_required_material(
                                &game_data,
                                material.item,
                                material.quantity,
                            ));
                        }
                    }
                }
            }

            let dialog = ui_state
                .manufacture
                .dialog_instance
                .get_mut(&dialog_assets, &ui_resources);
            if let Some(dialog) = dialog {
                let mut response_start_button = None;
                let mut response_close_button = None;
                let mut class_combo_changed = None;
                let mut item_combo_changed = None;

                egui::Window::new("Manufacture")
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
                                combo: &mut [
                                    (
                                        IID_COMBOBOX_CLASS,
                                        (
                                            &mut selected_class_index,
                                            0..class_names.len() as i32,
                                            &|index| {
                                                class_names
                                                    .get(index as usize)
                                                    .map(|name| name.to_string())
                                            },
                                        ),
                                    ),
                                    (
                                        IID_COMBOBOX_ITEM,
                                        (
                                            &mut selected_item_index,
                                            0..item_names.len() as i32,
                                            &|index| {
                                                item_names
                                                    .get(index as usize)
                                                    .map(|name| name.to_string())
                                            },
                                        ),
                                    ),
                                ],
                                combo_changed: &mut [
                                    (IID_COMBOBOX_CLASS, &mut class_combo_changed),
                                    (IID_COMBOBOX_ITEM, &mut item_combo_changed),
                                ],
                                response: &mut [
                                    (IID_BTN_START, &mut response_start_button),
                                    (IID_BTN_CLOSE, &mut response_close_button),
                                ],
                                ..Default::default()
                            },
                            |ui, _bindings| {
                                if let Some(sprite) = selected_item_preview_sprite.as_ref() {
                                    sprite.draw(
                                        ui,
                                        ui.min_rect().min
                                            + egui::vec2(
                                                MANUFACTURE_PREVIEW_SLOT_X,
                                                MANUFACTURE_PREVIEW_SLOT_Y,
                                            ),
                                    );
                                }

                                // Material slots - 4 slots for dragging materials from inventory
                                for slot_idx in 0..4usize {
                                    let slot_y = MANUFACTURE_MATERIAL_SLOT_Y
                                        + slot_idx as f32 * MANUFACTURE_MATERIAL_SLOT_STEP_Y;
                                    let text_y = MANUFACTURE_MATERIAL_NAME_Y
                                        + slot_idx as f32 * MANUFACTURE_MATERIAL_TEXT_STEP_Y;
                                    let pos = egui::pos2(MANUFACTURE_MATERIAL_SLOT_X, slot_y);

                                    if let Some(requirement) = required_materials.get(slot_idx) {
                                        ui.put(
                                            egui::Rect::from_min_size(
                                                ui.min_rect().min
                                                    + egui::vec2(
                                                        MANUFACTURE_MATERIAL_NAME_X,
                                                        text_y,
                                                    ),
                                                egui::vec2(MANUFACTURE_MATERIAL_NAME_WIDTH, 14.0),
                                            ),
                                            egui::Label::new(
                                                egui::RichText::new(&requirement.name)
                                                    .color(egui::Color32::YELLOW)
                                                    .font(egui::FontId::proportional(11.0)),
                                            ),
                                        );

                                        let count_rect = egui::Rect::from_min_size(
                                            ui.min_rect().min
                                                + egui::vec2(
                                                    MANUFACTURE_MATERIAL_COUNT_X,
                                                    MANUFACTURE_MATERIAL_COUNT_Y
                                                        + slot_idx as f32
                                                            * MANUFACTURE_MATERIAL_TEXT_STEP_Y,
                                                ),
                                            egui::vec2(MANUFACTURE_MATERIAL_COUNT_WIDTH, 14.0),
                                        );
                                        ui.painter().text(
                                            count_rect.right_top(),
                                            egui::Align2::RIGHT_TOP,
                                            format!("x{}", requirement.quantity),
                                            egui::FontId::proportional(11.0),
                                            egui::Color32::WHITE,
                                        );
                                    }

                                    if let Some(dropped) = ui_add_craft_item_slot(
                                        ui,
                                        DragAndDropId::CraftMaterial(slot_idx),
                                        pos,
                                        ui_state.manufacture.material_slots[slot_idx],
                                        &player,
                                        player_tooltip_data.as_ref(),
                                        &game_data,
                                        &ui_resources,
                                        &mut ui_state_dnd,
                                        required_materials.get(slot_idx).map(|r| r.quantity),
                                    ) {
                                        if let DragAndDropId::Inventory(inv_slot) = dropped {
                                            match validate_manufacture_material_drop(
                                                &game_data,
                                                &player,
                                                inv_slot,
                                                required_materials.get(slot_idx),
                                            ) {
                                                Ok(()) => {
                                                    ui_state.manufacture.material_slots[slot_idx] =
                                                        Some(inv_slot);
                                                }
                                                Err(reason) => {
                                                    chatbox_events.send(ChatboxEvent::System(
                                                        reason.to_string(),
                                                    ));
                                                }
                                            }
                                        }
                                    }
                                }

                                let success_rect = egui::Rect::from_min_size(
                                    ui.min_rect().min
                                        + egui::vec2(MANUFACTURE_SUCCESS_X, MANUFACTURE_SUCCESS_Y),
                                    egui::vec2(
                                        MANUFACTURE_SUCCESS_WIDTH,
                                        MANUFACTURE_SUCCESS_HEIGHT,
                                    ),
                                );
                                ui.painter().text(
                                    success_rect.right_top(),
                                    egui::Align2::RIGHT_TOP,
                                    selected_item_success_rate.to_string(),
                                    egui::FontId::proportional(11.0),
                                    egui::Color32::WHITE,
                                );

                                let mp_rect = egui::Rect::from_min_size(
                                    ui.min_rect().min
                                        + egui::vec2(MANUFACTURE_MP_X, MANUFACTURE_MP_Y),
                                    egui::vec2(MANUFACTURE_MP_WIDTH, MANUFACTURE_MP_HEIGHT),
                                );
                                ui.painter().text(
                                    mp_rect.right_top(),
                                    egui::Align2::RIGHT_TOP,
                                    required_mp_display.to_string(),
                                    egui::FontId::proportional(11.0),
                                    egui::Color32::WHITE,
                                );
                            },
                        );
                    });

                let class_changed = class_combo_changed.is_some()
                    || selected_class_index != ui_state.manufacture.selected_class_index;
                ui_state.manufacture.selected_class_index = selected_class_index;
                if class_changed {
                    ui_state.manufacture.selected_item_index = 0;
                } else {
                    ui_state.manufacture.selected_item_index = selected_item_index;
                }
                refresh_manufacture_filtered_items(&mut ui_state.manufacture);

                let selected_item_after =
                    selected_manufacture_item(&ui_state.manufacture).map(|(index, _)| index);
                if selected_item_before != selected_item_after || item_combo_changed.is_some() {
                    ui_state.manufacture.material_slots = [None; 4];
                }

                // Handle button responses
                if response_start_button.map_or(false, |r| r.clicked()) {
                    if let Some((_, (item_type, item_number, _))) =
                        selected_manufacture_item(&ui_state.manufacture)
                    {
                        let mut validation_error = None;
                        if let Some(base_item) = game_data
                            .items
                            .get_base_item(ItemReference::new(*item_type, *item_number))
                        {
                            if craft_skill_level < base_item.craft_skill_level {
                                validation_error =
                                    Some("Crafting failed: insufficient skill level.");
                            }
                        }
                        if validation_error.is_none() && player.mana_points.mp < required_mp_display
                        {
                            validation_error = Some("Crafting failed: insufficient MP.");
                        }

                        if validation_error.is_none() {
                            for (slot_idx, requirement) in required_materials.iter().enumerate() {
                                let Some(inv_slot) = ui_state.manufacture.material_slots[slot_idx]
                                else {
                                    validation_error = Some("Insert required materials.");
                                    break;
                                };
                                if let Err(reason) = validate_manufacture_material_drop(
                                    &game_data,
                                    &player,
                                    inv_slot,
                                    Some(requirement),
                                ) {
                                    validation_error = Some(reason);
                                    break;
                                }
                            }
                        }

                        if let Some(error) = validation_error {
                            chatbox_events.send(ChatboxEvent::System(error.to_string()));
                        } else if let Some(game_connection) = game_connection.as_ref() {
                            game_connection
                                .client_message_tx
                                .send(ClientMessage::CraftCreateItem {
                                    skill_slot,
                                    target_item_type: *item_type,
                                    target_item_number: *item_number,
                                    material_inventory_slots: ui_state
                                        .manufacture
                                        .material_slots
                                        .map(|s| {
                                            s.unwrap_or(ItemSlot::Inventory(
                                                InventoryPageType::Materials,
                                                0,
                                            ))
                                        }),
                                })
                                .ok();
                        }
                    }
                }

                if response_close_button.map_or(false, |r| r.clicked()) {
                    ui_state_windows.craft_manufacture_open = false;
                    ui_state_windows.craft_manufacture_skill_slot = None;
                    ui_state_windows.craft_manufacture_make_number = None;
                    ui_state.manufacture = UiCraftManufactureState::default();
                    ui_state.last_manufacture_skill = 0;
                    ui_state.last_manufacture_skill_level = 0;
                }
            }
        } else {
            chatbox_events.send(ChatboxEvent::System(
                "You don't have a crafting skill.".to_string(),
            ));
            ui_state_windows.craft_manufacture_open = false;
            ui_state_windows.craft_manufacture_skill_slot = None;
            ui_state_windows.craft_manufacture_make_number = None;
            ui_state.last_manufacture_skill = 0;
            ui_state.last_manufacture_skill_level = 0;
        }
    }

    // =================== UPGRADE WINDOW ===================
    if ui_state_windows.craft_upgrade_open {
        if ui_state.upgrade.source != ui_state_windows.craft_upgrade_source {
            ui_state.upgrade = UiCraftUpgradeState::default();
            ui_state.upgrade.source = ui_state_windows.craft_upgrade_source;
        }

        let mut skill_source = None;
        let mut npc_source = None;
        let mut show_cost_label = false;
        let mut upgrade_resource_cost = 0i64;

        match ui_state_windows.craft_upgrade_source {
            Some(UiUpgradeSource::Skill(skill_slot)) => {
                let mut craft_skill = validate_crafting_skill_slot(
                    &player.skill_list,
                    &game_data,
                    Some(skill_slot),
                    42..=42,
                    Some(42),
                );
                if craft_skill.is_none() {
                    craft_skill = find_crafting_skill(&player.skill_list, &game_data, 42..=42);
                    if let Some((resolved_skill_slot, _)) = craft_skill {
                        log::warn!(
                            "Upgrade craft context missing/invalid, falling back to first matching skill."
                        );
                        ui_state_windows.craft_upgrade_source =
                            Some(UiUpgradeSource::Skill(resolved_skill_slot));
                        ui_state.upgrade.source = ui_state_windows.craft_upgrade_source;
                    }
                }

                if let Some((resolved_skill_slot, _)) = craft_skill {
                    upgrade_resource_cost = player
                        .skill_list
                        .get_skill(resolved_skill_slot)
                        .and_then(|skill_id| game_data.skills.get_skill(skill_id))
                        .map_or(0, manufacture_required_mp)
                        as i64;
                    skill_source = Some(resolved_skill_slot);
                } else {
                    chatbox_events.send(ChatboxEvent::System(
                        "You don't have an upgrade skill.".to_string(),
                    ));
                    ui_state_windows.craft_upgrade_open = false;
                    ui_state_windows.craft_upgrade_source = None;
                    ui_state.upgrade = UiCraftUpgradeState::default();
                }
            }
            Some(UiUpgradeSource::Npc(client_entity_id)) => {
                let npc_in_range = client_entity_list
                    .get(client_entity_id)
                    .and_then(|entity| query_npc.get(entity).ok())
                    .map_or(false, |npc_position| {
                        player
                            .position
                            .position
                            .xy()
                            .distance(npc_position.position.xy())
                            <= 600.0
                    });
                if npc_in_range {
                    npc_source = Some(client_entity_id);
                    show_cost_label = true;
                } else {
                    ui_state_windows.craft_upgrade_open = false;
                    ui_state_windows.craft_upgrade_source = None;
                    ui_state.upgrade = UiCraftUpgradeState::default();
                }
            }
            None => {
                ui_state_windows.craft_upgrade_open = false;
                ui_state_windows.craft_upgrade_source = None;
                ui_state.upgrade = UiCraftUpgradeState::default();
            }
        }

        if skill_source.is_some() || npc_source.is_some() {
            let ui_state = &mut *ui_state;
            let upgrade_npc_cost = ui_state
                .upgrade
                .item_slot
                .and_then(|inv_slot| player.inventory.get_item(inv_slot))
                .and_then(|item| item.as_equipment())
                .and_then(|equipment| {
                    game_data
                        .items
                        .get_base_item(equipment.item)
                        .map(|base_item| upgrade_from_npc_price(base_item.quality, equipment.grade))
                });
            if npc_source.is_some() {
                upgrade_resource_cost = upgrade_npc_cost.map_or(0, |money| money.0);
            }

            let dialog = ui_state
                .upgrade
                .dialog_instance
                .get_mut(&dialog_assets, &ui_resources);
            if let Some(dialog) = dialog {
                let mut response_start_button = None;
                let mut response_close_button = None;
                let mut visibility_bindings = [(IID_TEXT_COST, show_cost_label)];

                egui::Window::new("Upgrade Equipment")
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
                                visible: &mut visibility_bindings,
                                response: &mut [
                                    (IID_BTN_START, &mut response_start_button),
                                    (IID_BTN_CLOSE, &mut response_close_button),
                                ],
                                ..Default::default()
                            },
                            |ui, _bindings| {
                                // Target equipment slot
                                let target_pos =
                                    egui::pos2(UPGRADE_TARGET_SLOT_X, UPGRADE_TARGET_SLOT_Y);
                                if let Some(dropped) = ui_add_craft_item_slot(
                                    ui,
                                    DragAndDropId::CraftUpgradeTarget,
                                    target_pos,
                                    ui_state.upgrade.item_slot,
                                    &player,
                                    player_tooltip_data.as_ref(),
                                    &game_data,
                                    &ui_resources,
                                    &mut ui_state_dnd,
                                    None,
                                ) {
                                    if let DragAndDropId::Inventory(inv_slot) = dropped {
                                        ui_state.upgrade.item_slot = Some(inv_slot);
                                    }
                                }

                                // Target item name (original centered rect)
                                if let Some(target_name) = build_upgrade_target_name(
                                    &game_data,
                                    &player,
                                    ui_state.upgrade.item_slot,
                                ) {
                                    let target_name_rect = egui::Rect::from_min_size(
                                        ui.min_rect().min
                                            + egui::vec2(UPGRADE_TARGET_NAME_X, UPGRADE_TARGET_NAME_Y),
                                        egui::vec2(UPGRADE_TARGET_NAME_WIDTH, UPGRADE_TARGET_NAME_HEIGHT),
                                    );
                                    draw_upgrade_centered_text(
                                        ui,
                                        target_name_rect,
                                        &target_name,
                                        egui::FontId::proportional(11.0),
                                        egui::Color32::WHITE,
                                    );
                                }

                                // 3 ingredient slots
                                let upgrade_requirements = build_upgrade_requirements(
                                    &game_data,
                                    &player,
                                    ui_state.upgrade.item_slot,
                                );
                                let upgrade_required_quantities =
                                    build_upgrade_required_quantities(&upgrade_requirements);
                                for slot_idx in 0..3usize {
                                    let pos = egui::pos2(
                                        UPGRADE_MATERIAL_SLOT_X,
                                        UPGRADE_MATERIAL_SLOT_Y
                                            + slot_idx as f32 * UPGRADE_MATERIAL_SLOT_STEP_Y,
                                    );

                                    if let Some(dropped) = ui_add_craft_item_slot(
                                        ui,
                                        DragAndDropId::CraftUpgradeIngredient(slot_idx),
                                        pos,
                                        ui_state.upgrade.ingredient_slots[slot_idx],
                                        &player,
                                        player_tooltip_data.as_ref(),
                                        &game_data,
                                        &ui_resources,
                                        &mut ui_state_dnd,
                                        upgrade_required_quantities[slot_idx],
                                    ) {
                                        if let DragAndDropId::Inventory(inv_slot) = dropped {
                                            if !upgrade_requirements.resolved {
                                                chatbox_events.send(ChatboxEvent::System(
                                                    "Cannot resolve upgrade requirements for this item."
                                                        .to_string(),
                                                ));
                                            } else {
                                                match validate_manufacture_material_drop(
                                                    &game_data,
                                                    &player,
                                                    inv_slot,
                                                    upgrade_requirements.requirements[slot_idx]
                                                        .as_ref(),
                                                ) {
                                                    Ok(()) => {
                                                        ui_state.upgrade.ingredient_slots[slot_idx] =
                                                            Some(inv_slot);
                                                    }
                                                    Err(reason) => {
                                                        chatbox_events.send(ChatboxEvent::System(
                                                            reason.to_string(),
                                                        ));
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }

                                let material_rows = build_upgrade_material_rows(
                                    &game_data,
                                    &upgrade_requirements,
                                    &player,
                                    &ui_state.upgrade.ingredient_slots,
                                );

                                for (row_index, row_data) in material_rows.iter().enumerate() {
                                    let row_y =
                                        row_index as f32 * UPGRADE_MATERIAL_TEXT_STEP_Y;
                                    let name_rect = egui::Rect::from_min_size(
                                        ui.min_rect().min
                                            + egui::vec2(
                                                UPGRADE_MATERIAL_NAME_X,
                                                UPGRADE_MATERIAL_NAME_Y + row_y,
                                            ),
                                        egui::vec2(
                                            UPGRADE_MATERIAL_NAME_WIDTH,
                                            UPGRADE_MATERIAL_NAME_HEIGHT,
                                        ),
                                    );
                                    let count_rect = egui::Rect::from_min_size(
                                        ui.min_rect().min
                                            + egui::vec2(
                                                UPGRADE_MATERIAL_COUNT_X,
                                                UPGRADE_MATERIAL_COUNT_Y + row_y,
                                            ),
                                        egui::vec2(
                                            UPGRADE_MATERIAL_COUNT_WIDTH,
                                            UPGRADE_MATERIAL_COUNT_HEIGHT,
                                        ),
                                    );

                                    if let Some((name, quantity)) = row_data.as_ref() {
                                        draw_upgrade_centered_text(
                                            ui,
                                            name_rect,
                                            name,
                                            egui::FontId::proportional(11.0),
                                            egui::Color32::WHITE,
                                        );
                                        draw_upgrade_right_aligned_text(
                                            ui,
                                            count_rect,
                                            &quantity.to_string(),
                                            egui::FontId::proportional(11.0),
                                            egui::Color32::WHITE,
                                        );
                                    }
                                }

                                // Success / cost numeric display (original compact style)
                                if let Some(inv_slot) = ui_state.upgrade.item_slot {
                                    if let Some(item) = player.inventory.get_item(inv_slot) {
                                        if let Some(equipment) = item.as_equipment() {
                                            let success_rate =
                                                (90i32 - equipment.grade as i32 * 8).clamp(10, 95);
                                            let success_rect = egui::Rect::from_min_size(
                                                ui.min_rect().min
                                                    + egui::vec2(UPGRADE_SUCCESS_X, UPGRADE_SUCCESS_Y),
                                                egui::vec2(
                                                    UPGRADE_SUCCESS_WIDTH,
                                                    UPGRADE_SUCCESS_HEIGHT,
                                                ),
                                            );
                                            draw_upgrade_right_aligned_text(
                                                ui,
                                                success_rect,
                                                &success_rate.to_string(),
                                                egui::FontId::proportional(11.0),
                                                egui::Color32::WHITE,
                                            );
                                        }
                                    }
                                }

                                let cost_rect = egui::Rect::from_min_size(
                                    ui.min_rect().min + egui::vec2(UPGRADE_COST_X, UPGRADE_COST_Y),
                                    egui::vec2(UPGRADE_COST_WIDTH, UPGRADE_COST_HEIGHT),
                                );
                                draw_upgrade_right_aligned_text(
                                    ui,
                                    cost_rect,
                                    &upgrade_resource_cost.to_string(),
                                    egui::FontId::proportional(11.0),
                                    egui::Color32::WHITE,
                                );

                                if DEBUG_UPGRADE_LAYOUT_OVERLAY {
                                    let overlay_stroke =
                                        egui::Stroke::new(1.0, egui::Color32::YELLOW);
                                    let target_slot_rect = egui::Rect::from_min_size(
                                        ui.min_rect().min
                                            + egui::vec2(UPGRADE_TARGET_SLOT_X, UPGRADE_TARGET_SLOT_Y),
                                        egui::vec2(40.0, 40.0),
                                    );
                                    ui.painter().rect_stroke(target_slot_rect, 0.0, overlay_stroke);

                                    for row_index in 0..3 {
                                        let slot_rect = egui::Rect::from_min_size(
                                            ui.min_rect().min
                                                + egui::vec2(
                                                    UPGRADE_MATERIAL_SLOT_X,
                                                    UPGRADE_MATERIAL_SLOT_Y
                                                        + row_index as f32
                                                            * UPGRADE_MATERIAL_SLOT_STEP_Y,
                                                ),
                                            egui::vec2(40.0, 40.0),
                                        );
                                        ui.painter().rect_stroke(slot_rect, 0.0, overlay_stroke);

                                        let row_y =
                                            row_index as f32 * UPGRADE_MATERIAL_TEXT_STEP_Y;
                                        let name_rect = egui::Rect::from_min_size(
                                            ui.min_rect().min
                                                + egui::vec2(
                                                    UPGRADE_MATERIAL_NAME_X,
                                                    UPGRADE_MATERIAL_NAME_Y + row_y,
                                                ),
                                            egui::vec2(
                                                UPGRADE_MATERIAL_NAME_WIDTH,
                                                UPGRADE_MATERIAL_NAME_HEIGHT,
                                            ),
                                        );
                                        ui.painter().rect_stroke(name_rect, 0.0, overlay_stroke);

                                        let count_rect = egui::Rect::from_min_size(
                                            ui.min_rect().min
                                                + egui::vec2(
                                                    UPGRADE_MATERIAL_COUNT_X,
                                                    UPGRADE_MATERIAL_COUNT_Y + row_y,
                                                ),
                                            egui::vec2(
                                                UPGRADE_MATERIAL_COUNT_WIDTH,
                                                UPGRADE_MATERIAL_COUNT_HEIGHT,
                                            ),
                                        );
                                        ui.painter().rect_stroke(count_rect, 0.0, overlay_stroke);
                                    }

                                    let target_name_rect = egui::Rect::from_min_size(
                                        ui.min_rect().min
                                            + egui::vec2(UPGRADE_TARGET_NAME_X, UPGRADE_TARGET_NAME_Y),
                                        egui::vec2(UPGRADE_TARGET_NAME_WIDTH, UPGRADE_TARGET_NAME_HEIGHT),
                                    );
                                    ui.painter().rect_stroke(target_name_rect, 0.0, overlay_stroke);
                                    let success_rect = egui::Rect::from_min_size(
                                        ui.min_rect().min
                                            + egui::vec2(UPGRADE_SUCCESS_X, UPGRADE_SUCCESS_Y),
                                        egui::vec2(UPGRADE_SUCCESS_WIDTH, UPGRADE_SUCCESS_HEIGHT),
                                    );
                                    ui.painter().rect_stroke(success_rect, 0.0, overlay_stroke);
                                    ui.painter().rect_stroke(cost_rect, 0.0, overlay_stroke);
                                }
                            },
                        );
                    });

                // Handle button responses
                if response_start_button.map_or(false, |r| r.clicked()) {
                    if let Some(item_slot) = ui_state.upgrade.item_slot {
                        let upgrade_requirements =
                            build_upgrade_requirements(&game_data, &player, Some(item_slot));
                        let mut validation_error: Option<&'static str> = None;

                        if !upgrade_requirements.resolved {
                            validation_error =
                                Some("Cannot refine this item: missing upgrade requirement data.");
                        }

                        if validation_error.is_none() {
                            for (slot_idx, requirement) in
                                upgrade_requirements.requirements.iter().enumerate()
                            {
                                match (
                                    requirement.as_ref(),
                                    ui_state.upgrade.ingredient_slots[slot_idx],
                                ) {
                                    (Some(requirement), Some(inv_slot)) => {
                                        if let Err(reason) = validate_manufacture_material_drop(
                                            &game_data,
                                            &player,
                                            inv_slot,
                                            Some(requirement),
                                        ) {
                                            validation_error = Some(reason);
                                            break;
                                        }
                                    }
                                    (Some(_), None) => {
                                        validation_error = Some("Insert required materials.");
                                        break;
                                    }
                                    (None, Some(_)) => {
                                        validation_error =
                                            Some("No material is required for this slot.");
                                        break;
                                    }
                                    (None, None) => {}
                                }
                            }
                        }

                        if validation_error.is_none() {
                            let mut quantity_by_slot: HashMap<ItemSlot, u32> = HashMap::new();
                            for (slot_idx, requirement) in
                                upgrade_requirements.requirements.iter().enumerate()
                            {
                                if let (Some(requirement), Some(inv_slot)) = (
                                    requirement.as_ref(),
                                    ui_state.upgrade.ingredient_slots[slot_idx],
                                ) {
                                    *quantity_by_slot.entry(inv_slot).or_insert(0) +=
                                        requirement.quantity;
                                }
                            }

                            for (inv_slot, required_quantity) in quantity_by_slot {
                                let Some(item) = player.inventory.get_item(inv_slot) else {
                                    validation_error = Some("Invalid inventory item.");
                                    break;
                                };

                                let Item::Stackable(stackable) = item else {
                                    validation_error = Some("Wrong material for this slot.");
                                    break;
                                };

                                if stackable.quantity < required_quantity {
                                    validation_error =
                                        Some("Not enough quantity for this material.");
                                    break;
                                }
                            }
                        }

                        if validation_error.is_none() {
                            if skill_source.is_some()
                                && player.mana_points.mp < upgrade_resource_cost as i32
                            {
                                validation_error = Some("Refining failed: insufficient MP.");
                            } else if npc_source.is_some()
                                && upgrade_npc_cost.map_or(true, |required_money| {
                                    player.inventory.money < required_money
                                })
                            {
                                validation_error = Some("Refining failed: insufficient money.");
                            }
                        }

                        if let Some(error) = validation_error {
                            chatbox_events.send(ChatboxEvent::System(error.to_string()));
                        } else if let Some(game_connection) = game_connection.as_ref() {
                            let default_slot = ItemSlot::Inventory(InventoryPageType::Materials, 0);
                            let ingredients = [
                                ui_state.upgrade.ingredient_slots[0].unwrap_or(default_slot),
                                ui_state.upgrade.ingredient_slots[1].unwrap_or(default_slot),
                                ui_state.upgrade.ingredient_slots[2].unwrap_or(default_slot),
                            ];
                            if let Some(skill_slot) = skill_source {
                                game_connection
                                    .client_message_tx
                                    .send(ClientMessage::CraftSkillUpgradeItem {
                                        skill_slot,
                                        item_slot,
                                        ingredients,
                                    })
                                    .ok();
                            } else if let Some(npc_entity_id) = npc_source {
                                game_connection
                                    .client_message_tx
                                    .send(ClientMessage::CraftNpcUpgradeItem {
                                        npc_entity_id,
                                        item_slot,
                                        ingredients,
                                    })
                                    .ok();
                            }
                        }
                    } else {
                        chatbox_events.send(ChatboxEvent::System(
                            "Place an equipment item to upgrade.".to_string(),
                        ));
                    }
                }

                if response_close_button.map_or(false, |r| r.clicked()) {
                    ui_state_windows.craft_upgrade_open = false;
                    ui_state_windows.craft_upgrade_source = None;
                    ui_state.upgrade = UiCraftUpgradeState::default();
                }
            }
        }
    }

    // =================== DISASSEMBLE WINDOW ===================
    if ui_state_windows.craft_disassemble_open {
        if ui_state.disassemble.source != ui_state_windows.craft_disassemble_source {
            ui_state.disassemble = UiCraftDisassembleState::default();
            ui_state.disassemble.source = ui_state_windows.craft_disassemble_source;
        }

        let mut skill_source = None;
        let mut npc_source = None;
        let mut show_cost_label = false;
        let mut resource_cost_display = 0i64;

        match ui_state_windows.craft_disassemble_source {
            Some(UiDisassembleSource::Skill(skill_slot)) => {
                let mut craft_skill = validate_crafting_skill_slot(
                    &player.skill_list,
                    &game_data,
                    Some(skill_slot),
                    41..=41,
                    Some(41),
                );
                if craft_skill.is_none() {
                    craft_skill = find_crafting_skill(&player.skill_list, &game_data, 41..=41);
                    if let Some((resolved_skill_slot, _)) = craft_skill {
                        log::warn!(
                            "Disassemble craft context missing/invalid, falling back to first matching skill."
                        );
                        ui_state_windows.craft_disassemble_source =
                            Some(UiDisassembleSource::Skill(resolved_skill_slot));
                        ui_state.disassemble.source = ui_state_windows.craft_disassemble_source;
                    }
                }

                if let Some((resolved_skill_slot, _)) = craft_skill {
                    resource_cost_display = player
                        .skill_list
                        .get_skill(resolved_skill_slot)
                        .and_then(|skill_id| game_data.skills.get_skill(skill_id))
                        .map_or(0, manufacture_required_mp)
                        as i64;
                    skill_source = Some(resolved_skill_slot);
                } else {
                    chatbox_events.send(ChatboxEvent::System(
                        "You don't have a disassembly skill.".to_string(),
                    ));
                    ui_state_windows.craft_disassemble_open = false;
                    ui_state_windows.craft_disassemble_source = None;
                    ui_state.disassemble = UiCraftDisassembleState::default();
                }
            }
            Some(UiDisassembleSource::Npc(client_entity_id)) => {
                let npc_in_range = client_entity_list
                    .get(client_entity_id)
                    .and_then(|entity| query_npc.get(entity).ok())
                    .map_or(false, |npc_position| {
                        player
                            .position
                            .position
                            .xy()
                            .distance(npc_position.position.xy())
                            <= 600.0
                    });
                if npc_in_range {
                    npc_source = Some(client_entity_id);
                    show_cost_label = true;
                } else {
                    ui_state_windows.craft_disassemble_open = false;
                    ui_state_windows.craft_disassemble_source = None;
                    ui_state.disassemble = UiCraftDisassembleState::default();
                }
            }
            None => {
                ui_state_windows.craft_disassemble_open = false;
                ui_state_windows.craft_disassemble_source = None;
                ui_state.disassemble = UiCraftDisassembleState::default();
            }
        }

        if skill_source.is_some() || npc_source.is_some() {
            let ui_state = &mut *ui_state;
            let mut input_item_name = None;
            let mut npc_required_money = None;
            let mut output_preview_rows: Vec<DisassemblePreviewRow> = Vec::new();
            if let Some(inv_slot) = ui_state.disassemble.item_slot {
                if let Some(item) = player.inventory.get_item(inv_slot) {
                    let item_ref = item.get_item_reference();
                    if let Some(base_item) = game_data.items.get_base_item(item_ref) {
                        input_item_name = Some(base_item.name.to_string());
                        npc_required_money = Some(disassemble_from_npc_price(base_item.quality));
                        if npc_source.is_some() {
                            resource_cost_display = npc_required_money.map_or(0, |money| money.0);
                        }
                        if let Some(product) =
                            get_product_with_fallback(&game_data, base_item.craft_material)
                        {
                            for (slot_idx, material) in product.materials.iter().take(4).enumerate()
                            {
                                output_preview_rows.push(build_disassemble_preview_row(
                                    &game_data,
                                    &ui_resources,
                                    item_ref,
                                    base_item.quality,
                                    product.raw_material_type,
                                    slot_idx,
                                    material,
                                ));
                            }
                        }
                    }
                }
            }

            let dialog = ui_state
                .disassemble
                .dialog_instance
                .get_mut(&dialog_assets, &ui_resources);
            if let Some(dialog) = dialog {
                let mut response_start_button = None;
                let mut response_close_button = None;
                let mut visibility_bindings = [(IID_TEXT_COST, show_cost_label)];

                egui::Window::new("Disassemble")
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
                                visible: &mut visibility_bindings,
                                response: &mut [
                                    (IID_BTN_START, &mut response_start_button),
                                    (IID_BTN_CLOSE, &mut response_close_button),
                                ],
                                ..Default::default()
                            },
                            |ui, _bindings| {
                                let input_pos =
                                    egui::pos2(SEPARATION_INPUT_SLOT_X, SEPARATION_INPUT_SLOT_Y);
                                if let Some(dropped) = ui_add_craft_item_slot(
                                    ui,
                                    DragAndDropId::CraftDisassembleInput,
                                    input_pos,
                                    ui_state.disassemble.item_slot,
                                    &player,
                                    player_tooltip_data.as_ref(),
                                    &game_data,
                                    &ui_resources,
                                    &mut ui_state_dnd,
                                    None,
                                ) {
                                    if let DragAndDropId::Inventory(inv_slot) = dropped {
                                        ui_state.disassemble.item_slot = Some(inv_slot);
                                    }
                                }

                                if let Some(item_name) = input_item_name.as_ref() {
                                    ui.put(
                                        egui::Rect::from_min_size(
                                            ui.min_rect().min
                                                + egui::vec2(
                                                    SEPARATION_INPUT_NAME_X,
                                                    SEPARATION_INPUT_NAME_Y,
                                                ),
                                            egui::vec2(
                                                SEPARATION_INPUT_NAME_WIDTH,
                                                SEPARATION_INPUT_NAME_HEIGHT,
                                            ),
                                        ),
                                        egui::Label::new(
                                            egui::RichText::new(item_name)
                                                .color(egui::Color32::YELLOW)
                                                .font(egui::FontId::proportional(11.0)),
                                        ),
                                    );
                                }

                                for (row_index, row) in output_preview_rows.iter().enumerate() {
                                    let row_index = row_index as f32;
                                    let slot_pos = ui.min_rect().min
                                        + egui::vec2(
                                            SEPARATION_OUTPUT_SLOT_X,
                                            SEPARATION_OUTPUT_SLOT_Y
                                                + row_index * SEPARATION_OUTPUT_SLOT_STEP_Y,
                                        );
                                    if let Some(sprite) = row.icon_sprite.as_ref() {
                                        sprite.draw(ui, slot_pos);
                                    }

                                    let name_rect = egui::Rect::from_min_size(
                                        ui.min_rect().min
                                            + egui::vec2(
                                                SEPARATION_OUTPUT_NAME_X,
                                                SEPARATION_OUTPUT_NAME_Y
                                                    + row_index * SEPARATION_OUTPUT_TEXT_STEP_Y,
                                            ),
                                        egui::vec2(
                                            SEPARATION_OUTPUT_NAME_WIDTH,
                                            SEPARATION_OUTPUT_NAME_HEIGHT,
                                        ),
                                    );
                                    ui.put(
                                        name_rect,
                                        egui::Label::new(
                                            egui::RichText::new(&row.name)
                                                .color(egui::Color32::YELLOW)
                                                .font(egui::FontId::proportional(11.0)),
                                        ),
                                    );

                                    let count_rect =
                                        get_separation_output_count_rect(ui, row_index);
                                    let count_text =
                                        format_disassemble_range_text(row.range_min, row.range_max);
                                    let count_font = pick_separation_output_count_font(
                                        ui,
                                        count_rect,
                                        &count_text,
                                    );
                                    let count_painter = ui.painter().with_clip_rect(count_rect);
                                    count_painter.text(
                                        count_rect.center()
                                            + egui::vec2(
                                                SEPARATION_OUTPUT_COUNT_NUDGE_X,
                                                SEPARATION_OUTPUT_COUNT_BASELINE_NUDGE_Y,
                                            ),
                                        egui::Align2::CENTER_CENTER,
                                        count_text,
                                        count_font,
                                        egui::Color32::WHITE,
                                    );
                                }

                                let mp_rect = egui::Rect::from_min_size(
                                    ui.min_rect().min
                                        + egui::vec2(SEPARATION_MP_X, SEPARATION_MP_Y),
                                    egui::vec2(SEPARATION_MP_WIDTH, SEPARATION_MP_HEIGHT),
                                );
                                ui.painter().text(
                                    mp_rect.right_top(),
                                    egui::Align2::RIGHT_TOP,
                                    resource_cost_display.to_string(),
                                    egui::FontId::proportional(11.0),
                                    egui::Color32::WHITE,
                                );
                            },
                        );
                    });

                if response_start_button.map_or(false, |r| r.clicked()) {
                    if let Some(item_slot) = ui_state.disassemble.item_slot {
                        if let Some(game_connection) = game_connection.as_ref() {
                            if let Some(skill_slot) = skill_source {
                                game_connection
                                    .client_message_tx
                                    .send(ClientMessage::CraftSkillDisassemble {
                                        skill_slot,
                                        item_slot,
                                    })
                                    .ok();
                            } else if let Some(npc_entity_id) = npc_source {
                                if let Some(required_money) = npc_required_money {
                                    if player.inventory.money < required_money {
                                        chatbox_events.send(ChatboxEvent::System(
                                            "Disassembly failed: insufficient money.".to_string(),
                                        ));
                                    } else {
                                        game_connection
                                            .client_message_tx
                                            .send(ClientMessage::CraftNpcDisassemble {
                                                npc_entity_id,
                                                item_slot,
                                            })
                                            .ok();
                                    }
                                } else {
                                    chatbox_events.send(ChatboxEvent::System(
                                        "That item can't be disassembled.".to_string(),
                                    ));
                                }
                            }
                        }
                    } else {
                        chatbox_events.send(ChatboxEvent::System(
                            "Place an item to disassemble.".to_string(),
                        ));
                    }
                }

                if response_close_button.map_or(false, |r| r.clicked()) {
                    ui_state_windows.craft_disassemble_open = false;
                    ui_state_windows.craft_disassemble_source = None;
                    ui_state.disassemble = UiCraftDisassembleState::default();
                }
            }
        }
    }
}
