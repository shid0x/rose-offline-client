use bevy::prelude::{Assets, EventWriter, Local, Query, Res, ResMut, With, World};
use bevy_egui::{egui, EguiContexts};

use rose_data::{Item, QuestData};
use rose_game_common::{
    components::{ActiveQuest, QuestState},
    messages::client::ClientMessage,
};

use crate::{
    components::PlayerCharacter,
    events::MessageBoxEvent,
    resources::{
        ConversationDialogState, GameConnection, GameData, UiResources, UiSpriteSheetType,
    },
    ui::{
        tooltips::{PlayerTooltipQuery, PlayerTooltipQueryItem},
        ui_add_item_tooltip,
        widgets::{DataBindings, Dialog, Widget},
        DragAndDropId, DragAndDropSlot, UiSoundEvent, UiStateWindows,
    },
};

use super::DialogInstance;

const IID_BTN_ABANDON: i32 = 50;
const IID_BTN_CLOSE: i32 = 10;
// const IID_BTN_ICONIZE: i32 = 11;
const IID_BTN_MINIMIZE: i32 = 113;
const IID_BTN_MAXIMIZE: i32 = 114;
const IID_ZLIST_QUEST: i32 = 20;
const IID_ZLIST_SCROLLBAR: i32 = 21;
const IID_LIST_QUESTINFO: i32 = 30;
// const IID_ZLIST_ITEM: i32 = 99;
// const IID_PANE_QUESTLIST: i32 = 100;
const IID_PANE_QUESTINFO: i32 = 200;

struct VisibleQuestEntry<'a> {
    slot: usize,
    active_quest: &'a ActiveQuest,
    quest_data: &'a QuestData,
}

fn format_quest_abandon_confirm(template: &str, quest_name: &str) -> String {
    if template.contains("%s") {
        template.replacen("%s", quest_name, 1)
    } else if template.contains("{}") {
        template.replacen("{}", quest_name, 1)
    } else {
        template.to_string()
    }
}

fn ui_add_quest_item_slot(
    ui: &mut egui::Ui,
    pos: egui::Pos2,
    player_tooltip_data: Option<&PlayerTooltipQueryItem>,
    item: Option<&Item>,
    game_data: &GameData,
    ui_resources: &UiResources,
) {
    let mut dragged_item = None;
    let mut dropped_item = None;
    let response = ui
        .allocate_ui_at_rect(
            egui::Rect::from_min_size(pos, egui::vec2(40.0, 40.0)),
            |ui| {
                egui::Widget::ui(
                    DragAndDropSlot::with_item(
                        DragAndDropId::NotDraggable,
                        item,
                        None,
                        game_data,
                        ui_resources,
                        |_| false,
                        &mut dragged_item,
                        &mut dropped_item,
                        [40.0, 40.0],
                    ),
                    ui,
                )
            },
        )
        .inner;

    if let Some(item) = item {
        response.on_hover_ui(|ui| {
            ui_add_item_tooltip(ui, game_data, player_tooltip_data, item);
        });
    }
}

pub struct UiQuestListState {
    pub dialog_instance: DialogInstance,
    pub scroll_index: i32,
    pub selected_index: i32,
    pub minimised: bool,
}

impl Default for UiQuestListState {
    fn default() -> Self {
        Self {
            dialog_instance: DialogInstance::new("DLGQUEST.XML"),
            scroll_index: 0,
            selected_index: 0,
            minimised: false,
        }
    }
}

pub fn ui_quest_list_system(
    mut ui_state: Local<UiQuestListState>,
    mut egui_context: EguiContexts,
    mut ui_state_windows: ResMut<UiStateWindows>,
    mut ui_sound_events: EventWriter<UiSoundEvent>,
    mut message_box_events: EventWriter<MessageBoxEvent>,
    query_player: Query<&QuestState, With<PlayerCharacter>>,
    query_player_tooltip: Query<PlayerTooltipQuery, With<PlayerCharacter>>,
    game_connection: Option<Res<GameConnection>>,
    conversation_dialog_state: Res<ConversationDialogState>,
    game_data: Res<GameData>,
    ui_resources: Res<UiResources>,
    dialog_assets: Res<Assets<Dialog>>,
) {
    let ui_state = &mut *ui_state;
    let dialog = if let Some(dialog) = ui_state
        .dialog_instance
        .get_mut(&dialog_assets, &ui_resources)
    {
        dialog
    } else {
        return;
    };
    let player_quest_state = if let Ok(player) = query_player.get_single() {
        player
    } else {
        return;
    };
    let player_tooltip_data = query_player_tooltip.get_single().ok();
    let visible_quests: Vec<_> = player_quest_state
        .active_quests
        .iter()
        .enumerate()
        .filter_map(|(slot, active_quest)| {
            let active_quest = active_quest.as_ref()?;
            let quest_data = game_data.quests.get_quest_data(active_quest.quest_id)?;
            Some(VisibleQuestEntry {
                slot,
                active_quest,
                quest_data,
            })
        })
        .collect();

    let listbox_extent = if let Some(Widget::ZListbox(listbox)) = dialog.get_widget(IID_ZLIST_QUEST)
    {
        listbox.extent
    } else {
        1
    };
    let num_quests = visible_quests.len();
    let max_scroll_index = (num_quests as i32 - listbox_extent).max(0);
    ui_state.scroll_index = ui_state.scroll_index.clamp(0, max_scroll_index);
    ui_state.selected_index = if num_quests == 0 {
        0
    } else {
        ui_state.selected_index.clamp(0, num_quests as i32 - 1)
    };

    let selected_quest = visible_quests.get(ui_state.selected_index as usize);
    let show_abandon_button = selected_quest.is_some();
    let abandon_button_enabled = show_abandon_button && !conversation_dialog_state.is_open;
    let scrollbar_range = 0..num_quests as i32;

    let mut response_abandon_button = None;
    let mut response_close_button = None;
    let mut response_minimise_button = None;
    let mut response_maximise_button = None;
    let is_minimised = ui_state.minimised;

    egui::Window::new("Quest List")
        .frame(egui::Frame::none())
        .open(&mut ui_state_windows.quest_list_open)
        .title_bar(false)
        .resizable(false)
        .default_width(dialog.width)
        .default_height(dialog.height)
        .show(egui_context.ctx_mut(), |ui| {
            dialog.draw(
                ui,
                DataBindings {
                    sound_events: Some(&mut ui_sound_events),
                    visible: &mut [
                        (IID_BTN_ABANDON, show_abandon_button),
                        (IID_ZLIST_SCROLLBAR, !is_minimised),
                        (IID_ZLIST_QUEST, !is_minimised),
                        (IID_BTN_MINIMIZE, !is_minimised),
                        (IID_BTN_MAXIMIZE, is_minimised),
                    ],
                    enabled: &mut [(IID_BTN_ABANDON, abandon_button_enabled)],
                    scroll: &mut [(
                        IID_ZLIST_QUEST,
                        (&mut ui_state.scroll_index, scrollbar_range, listbox_extent),
                    )],
                    zlist: &mut [(
                        IID_ZLIST_QUEST,
                        (&mut ui_state.selected_index, &|ui, index, selected| {
                            let item_height = 24.0;
                            let item_width = 174.0;
                            let y_offset = index as f32 * item_height;
                            let rect = egui::Rect::from_min_size(
                                ui.min_rect().min + egui::vec2(0.0, y_offset),
                                egui::vec2(item_width, item_height),
                            );
                            let response = ui.allocate_rect(rect, egui::Sense::click());

                            if let Some(visible_quest) = visible_quests.get(index as usize) {
                                if visible_quest.quest_data.icon_id != 0 {
                                    if let Some(icon_sprite) = ui_resources.get_sprite_by_index(
                                        UiSpriteSheetType::StateIcon,
                                        visible_quest.quest_data.icon_id as usize,
                                    ) {
                                        icon_sprite.draw(ui, rect.min + egui::vec2(3.0, 1.0));
                                    }
                                }

                                ui.painter().text(
                                    rect.min + egui::vec2(28.0, 4.0),
                                    egui::Align2::LEFT_TOP,
                                    visible_quest.quest_data.name,
                                    egui::FontId::default(),
                                    if selected {
                                        egui::Color32::YELLOW
                                    } else {
                                        egui::Color32::WHITE
                                    },
                                );
                            }

                            response
                        }),
                    )],
                    response: &mut [
                        (IID_BTN_ABANDON, &mut response_abandon_button),
                        (IID_BTN_CLOSE, &mut response_close_button),
                        (IID_BTN_MINIMIZE, &mut response_minimise_button),
                        (IID_BTN_MAXIMIZE, &mut response_maximise_button),
                    ],
                    ..Default::default()
                },
                |ui, bindings| {
                    let selected_quest_index = bindings
                        .get_zlist_selected_index(IID_ZLIST_QUEST)
                        .unwrap_or(0);

                    if let Some(selected_quest) = visible_quests.get(selected_quest_index as usize)
                    {
                        let rect_info = if let Some(Widget::Pane(pane)) =
                            dialog.get_widget(IID_PANE_QUESTINFO)
                        {
                            pane.widget_rect(ui.min_rect().min)
                        } else {
                            ui.min_rect()
                        };

                        if selected_quest.quest_data.icon_id != 0 {
                            if let Some(icon_sprite) = ui_resources.get_sprite_by_index(
                                UiSpriteSheetType::StateIcon,
                                selected_quest.quest_data.icon_id as usize,
                            ) {
                                icon_sprite.draw(ui, rect_info.min + egui::vec2(18.0, 35.0));
                            }
                        }

                        ui.allocate_ui_at_rect(rect_info.translate(egui::vec2(43.0, 38.0)), |ui| {
                            ui.horizontal_top(|ui| {
                                ui.add(egui::Label::new(
                                    egui::RichText::new(selected_quest.quest_data.name)
                                        .color(egui::Color32::YELLOW),
                                ));
                            })
                        });

                        if let Some(Widget::Listbox(listbox)) =
                            dialog.get_widget(IID_LIST_QUESTINFO)
                        {
                            let rect = listbox.widget_rect(rect_info.min);

                            ui.allocate_ui_at_rect(rect, |ui| {
                                egui::ScrollArea::vertical().auto_shrink([false; 2]).show(
                                    ui,
                                    |ui| {
                                        ui.label(selected_quest.quest_data.description);
                                    },
                                );
                            });
                        }

                        const QUEST_ITEM_SLOT_POS: [egui::Vec2; 6] = [
                            egui::vec2(10.0, 176.0),
                            egui::vec2(51.0, 176.0),
                            egui::vec2(92.0, 176.0),
                            egui::vec2(133.0, 176.0),
                            egui::vec2(174.0, 176.0),
                            egui::vec2(211.0, 176.0),
                        ];

                        for (i, item) in selected_quest.active_quest.items.iter().enumerate() {
                            ui_add_quest_item_slot(
                                ui,
                                rect_info.min + QUEST_ITEM_SLOT_POS[i],
                                player_tooltip_data.as_ref(),
                                item.as_ref(),
                                &game_data,
                                &ui_resources,
                            );
                        }
                    }
                },
            );
        });

    if response_close_button.map_or(false, |r| r.clicked()) {
        ui_state_windows.quest_list_open = false;
    }

    if response_abandon_button.map_or(false, |r| r.clicked()) {
        if let (Some(selected_quest), Some(game_connection)) =
            (selected_quest, game_connection.as_ref())
        {
            let client_message_tx = game_connection.client_message_tx.clone();
            let quest_slot = selected_quest.slot;
            let quest_id = selected_quest.active_quest.quest_id;
            message_box_events.send(MessageBoxEvent::Show {
                message: format_quest_abandon_confirm(
                    game_data.client_strings.quest_abandon_confirm,
                    selected_quest.quest_data.name,
                ),
                modal: true,
                ok: Some(Box::new(move |commands| {
                    let client_message_tx = client_message_tx.clone();
                    commands.add(move |_world: &mut World| {
                        client_message_tx
                            .send(ClientMessage::QuestDelete {
                                slot: quest_slot,
                                quest_id,
                            })
                            .ok();
                    });
                })),
                cancel: Some(Box::new(|_| {})),
            });
        }
    }

    if response_minimise_button.map_or(false, |r| r.clicked()) {
        ui_state.minimised = true;

        if let Some(Widget::Pane(pane)) = dialog.get_widget_mut(IID_PANE_QUESTINFO) {
            pane.y = 56.0;
        }
    }

    if response_maximise_button.map_or(false, |r| r.clicked()) {
        ui_state.minimised = false;

        if let Some(Widget::Pane(pane)) = dialog.get_widget_mut(IID_PANE_QUESTINFO) {
            pane.y = 171.0;
        }
    }
}
