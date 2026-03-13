use bevy_egui::egui;
use serde::Deserialize;

use crate::resources::{UiResources, UiSprite};

use super::{DataBindings, DrawWidget, LoadWidget};

#[derive(Clone, Default, Deserialize)]
#[serde(rename = "COMBOBOX")]
#[serde(default)]
pub struct ComboBox {
    #[serde(rename = "ID")]
    pub id: i32,
    #[serde(rename = "NAME")]
    pub name: String,
    #[serde(rename = "X")]
    pub x: f32,
    #[serde(rename = "Y")]
    pub y: f32,
    #[serde(rename = "OFFSETX")]
    pub offset_x: f32,
    #[serde(rename = "OFFSETY")]
    pub offset_y: f32,
    #[serde(rename = "WIDTH")]
    pub width: f32,
    #[serde(rename = "HEIGHT")]
    pub height: f32,
    #[serde(rename = "OWNERDRAW", alias = "OWNERDROW", alias = "OWNERDRAR")]
    pub owner_draw: i32,

    #[serde(rename = "$value", default)]
    children: Vec<ComboChild>,

    #[serde(skip)]
    drop_button: Option<ComboButton>,
    #[serde(skip)]
    top_image: Option<ComboImage>,
    #[serde(skip)]
    middle_image: Option<ComboImage>,
    #[serde(skip)]
    bottom_image: Option<ComboImage>,
    #[serde(skip)]
    listbox: Option<ComboListbox>,
}

#[derive(Clone, Deserialize)]
enum ComboChild {
    #[serde(rename = "BUTTON")]
    Button(ComboButton),
    #[serde(rename = "TOPIMAGE")]
    TopImage(ComboImage),
    #[serde(rename = "MIDDLEIMAGE")]
    MiddleImage(ComboImage),
    #[serde(rename = "BOTTOMIMAGE")]
    BottomImage(ComboImage),
    #[serde(rename = "LISTBOX")]
    Listbox(ComboListbox),
    #[serde(rename = "JLISTBOX")]
    JListbox(ComboListbox),
    #[serde(other)]
    Unknown,
}

#[derive(Clone, Default, Deserialize)]
#[serde(rename = "BUTTON")]
#[serde(default)]
struct ComboButton {
    #[serde(rename = "X")]
    pub x: f32,
    #[serde(rename = "Y")]
    pub y: f32,
    #[serde(rename = "OFFSETX")]
    pub offset_x: f32,
    #[serde(rename = "OFFSETY")]
    pub offset_y: f32,
    #[serde(rename = "WIDTH")]
    pub width: f32,
    #[serde(rename = "HEIGHT")]
    pub height: f32,
    #[serde(rename = "MODULEID")]
    pub module_id: i32,
    #[serde(rename = "NORMALGID")]
    pub normal_sprite_name: String,
    #[serde(rename = "OVERGID")]
    pub over_sprite_name: String,
    #[serde(rename = "DOWNGID")]
    pub down_sprite_name: String,

    #[serde(skip)]
    pub normal_sprite: Option<UiSprite>,
    #[serde(skip)]
    pub over_sprite: Option<UiSprite>,
    #[serde(skip)]
    pub down_sprite: Option<UiSprite>,
}

#[derive(Clone, Default, Deserialize)]
#[serde(rename = "TOPIMAGE")]
#[serde(default)]
struct ComboImage {
    #[serde(rename = "X")]
    pub x: f32,
    #[serde(rename = "Y")]
    pub y: f32,
    #[serde(rename = "OFFSETX")]
    pub offset_x: f32,
    #[serde(rename = "OFFSETY")]
    pub offset_y: f32,
    #[serde(rename = "WIDTH")]
    pub width: f32,
    #[serde(rename = "HEIGHT")]
    pub height: f32,
    #[serde(rename = "MODULEID")]
    pub module_id: i32,
    #[serde(rename = "GID")]
    pub sprite_name: String,

    #[serde(skip)]
    pub sprite: Option<UiSprite>,
}

#[derive(Clone, Default, Deserialize)]
#[serde(rename = "LISTBOX")]
#[serde(default)]
struct ComboListbox {
    #[serde(rename = "X")]
    pub x: f32,
    #[serde(rename = "Y")]
    pub y: f32,
    #[serde(rename = "WIDTH")]
    pub width: f32,
    #[serde(rename = "HEIGHT")]
    pub height: f32,
    #[serde(rename = "ITEMHEIGHT")]
    pub item_height: i32,
    #[serde(rename = "CHARHEIGHT")]
    pub char_height: i32,
    #[serde(rename = "LINESPACE")]
    pub line_space: i32,
    #[serde(rename = "EXTENT")]
    pub extent: i32,
}

impl ComboButton {
    fn load_widget(&mut self, ui_resources: &UiResources) {
        self.normal_sprite = ui_resources.get_sprite(self.module_id, &self.normal_sprite_name);
        self.over_sprite = ui_resources.get_sprite(self.module_id, &self.over_sprite_name);
        self.down_sprite = ui_resources.get_sprite(self.module_id, &self.down_sprite_name);
    }
}

impl ComboImage {
    fn load_widget(&mut self, ui_resources: &UiResources) {
        self.sprite = ui_resources.get_sprite(self.module_id, &self.sprite_name);
    }

    fn position(&self, base: egui::Pos2) -> egui::Pos2 {
        base + egui::vec2(self.x + self.offset_x, self.y + self.offset_y)
    }
}

impl ComboListbox {
    fn resolved_item_height(&self) -> f32 {
        if self.item_height > 0 {
            self.item_height as f32
        } else if self.char_height > 0 {
            (self.char_height + self.line_space.max(0)) as f32
        } else {
            13.0
        }
    }

    fn resolved_visible_count(&self, num_items: usize) -> usize {
        if self.extent > 0 {
            (self.extent as usize).max(1)
        } else if self.height > 0.0 {
            (self.height / self.resolved_item_height()).floor().max(1.0) as usize
        } else {
            num_items.max(1).min(8)
        }
    }
}

widget_to_rect! { ComboBox }

impl LoadWidget for ComboBox {
    fn load_widget(&mut self, ui_resources: &UiResources) {
        self.drop_button = None;
        self.top_image = None;
        self.middle_image = None;
        self.bottom_image = None;
        self.listbox = None;

        for child in self.children.iter_mut() {
            match child {
                ComboChild::Button(button) => {
                    button.load_widget(ui_resources);
                    if self.drop_button.is_none() {
                        self.drop_button = Some(button.clone());
                    }
                }
                ComboChild::TopImage(image) => {
                    image.load_widget(ui_resources);
                    self.top_image = Some(image.clone());
                }
                ComboChild::MiddleImage(image) => {
                    image.load_widget(ui_resources);
                    self.middle_image = Some(image.clone());
                }
                ComboChild::BottomImage(image) => {
                    image.load_widget(ui_resources);
                    self.bottom_image = Some(image.clone());
                }
                ComboChild::Listbox(listbox) | ComboChild::JListbox(listbox) => {
                    self.listbox = Some(listbox.clone());
                }
                ComboChild::Unknown => {}
            }
        }
    }
}

impl DrawWidget for ComboBox {
    fn draw_widget(&self, ui: &mut egui::Ui, bindings: &mut DataBindings) {
        if !bindings.get_visible(self.id) || !bindings.has_combo(self.id) {
            return;
        }

        let mut items = Vec::new();
        let selected_index;
        {
            let Some((selected, range, get_item_text)) = bindings.get_combo(self.id) else {
                return;
            };

            if range.is_empty() {
                *selected = 0;
            } else {
                if *selected < range.start {
                    *selected = range.start;
                }
                if *selected >= range.end {
                    *selected = range.end - 1;
                }
            }
            selected_index = *selected;

            for index in range {
                if let Some(text) = get_item_text(index) {
                    items.push((index, text));
                }
            }
        }

        let rect = self.widget_rect(ui.min_rect().min);
        let enabled = bindings.get_enabled(self.id) && !items.is_empty();
        let mut clickable_rect = rect;

        let drop_button_rect = self.drop_button.as_ref().map(|button| {
            let min = rect.min + egui::vec2(button.x + button.offset_x, button.y + button.offset_y);
            let size = egui::vec2(button.width, button.height);
            let button_rect = egui::Rect::from_min_size(min, size);
            clickable_rect = clickable_rect.union(button_rect);
            button_rect
        });

        let response = ui.allocate_rect(
            clickable_rect,
            if enabled {
                egui::Sense::click()
            } else {
                egui::Sense::hover()
            },
        );

        let selected_text = items
            .iter()
            .find(|(index, _)| *index == selected_index)
            .map(|(_, text)| text.as_str())
            .unwrap_or("");
        ui.painter().text(
            egui::pos2(rect.left() + 2.0, rect.center().y),
            egui::Align2::LEFT_CENTER,
            selected_text,
            egui::FontId::proportional(12.0),
            egui::Color32::WHITE,
        );

        if let (Some(button), Some(button_rect)) = (self.drop_button.as_ref(), drop_button_rect) {
            let sprite = if !enabled {
                button.normal_sprite.as_ref()
            } else if response.is_pointer_button_down_on() {
                button
                    .down_sprite
                    .as_ref()
                    .or(button.over_sprite.as_ref())
                    .or(button.normal_sprite.as_ref())
            } else if response.hovered() {
                button
                    .over_sprite
                    .as_ref()
                    .or(button.normal_sprite.as_ref())
                    .or(button.down_sprite.as_ref())
            } else {
                button
                    .normal_sprite
                    .as_ref()
                    .or(button.over_sprite.as_ref())
                    .or(button.down_sprite.as_ref())
            };

            if let Some(sprite) = sprite {
                sprite.draw(ui, button_rect.min);
            }
        }

        let popup_state_id = ui.make_persistent_id(("combo_popup_state", self.id));
        let mut popup_open = ui
            .ctx()
            .data(|data| data.get_temp::<bool>(popup_state_id).unwrap_or(false));

        if enabled && response.clicked() {
            popup_open = !popup_open;
        }

        let mut popup_rect = egui::Rect::NOTHING;
        let mut pending_selection = None;

        if popup_open {
            let listbox = self.listbox.as_ref();
            let item_height = listbox
                .map(|listbox| listbox.resolved_item_height())
                .unwrap_or(13.0)
                .max(1.0);
            let effective_row_height = item_height.max(ui.spacing().interact_size.y);
            let visible_items = listbox
                .map(|listbox| listbox.resolved_visible_count(items.len()))
                .unwrap_or_else(|| items.len().max(1).min(8))
                .max(1);
            let popup_width = listbox
                .map(|listbox| {
                    if listbox.width > 0.0 {
                        listbox.width
                    } else {
                        rect.width()
                    }
                })
                .unwrap_or_else(|| rect.width());
            let top_height = self.top_image.as_ref().map_or(0.0, |image| image.height);
            let bottom_height = self.bottom_image.as_ref().map_or(0.0, |image| image.height);
            let list_height = visible_items as f32 * effective_row_height;
            let popup_size = egui::vec2(popup_width, top_height + list_height + bottom_height);

            let list_offset = listbox
                .map(|listbox| egui::vec2(listbox.x, listbox.y))
                .unwrap_or(egui::Vec2::ZERO);
            let mut popup_pos = rect.min + egui::vec2(list_offset.x, rect.height() + list_offset.y);

            let screen_rect = ui.ctx().input(|input| input.screen_rect());
            if popup_pos.x + popup_size.x > screen_rect.right() {
                popup_pos.x = screen_rect.right() - popup_size.x;
            }
            if popup_pos.x < screen_rect.left() {
                popup_pos.x = screen_rect.left();
            }
            if popup_pos.y + popup_size.y > screen_rect.bottom() {
                popup_pos.y = rect.top() - popup_size.y;
            }
            if popup_pos.y < screen_rect.top() {
                popup_pos.y = screen_rect.top();
            }

            let popup_area_id = ui.make_persistent_id(("combo_popup_area", self.id));
            let popup_area = egui::Area::new(popup_area_id)
                .order(egui::Order::Foreground)
                .fixed_pos(popup_pos)
                .show(ui.ctx(), |ui| {
                    let popup_rect = egui::Rect::from_min_size(ui.min_rect().min, popup_size);

                    if let Some(top_image) = self.top_image.as_ref() {
                        if let Some(sprite) = top_image.sprite.as_ref() {
                            sprite.draw(ui, top_image.position(popup_rect.min));
                        }
                    }
                    if let Some(middle_image) = self.middle_image.as_ref() {
                        if let Some(sprite) = middle_image.sprite.as_ref() {
                            let middle_rect = egui::Rect::from_min_size(
                                popup_rect.min
                                    + egui::vec2(
                                        middle_image.x + middle_image.offset_x,
                                        top_height + middle_image.y + middle_image.offset_y,
                                    ),
                                egui::vec2(middle_image.width.max(1.0), list_height),
                            );
                            sprite.draw_stretched(ui, middle_rect);
                        }
                    }
                    if let Some(bottom_image) = self.bottom_image.as_ref() {
                        if let Some(sprite) = bottom_image.sprite.as_ref() {
                            let pos = popup_rect.min
                                + egui::vec2(
                                    bottom_image.x + bottom_image.offset_x,
                                    popup_size.y - bottom_height
                                        + bottom_image.y
                                        + bottom_image.offset_y,
                                );
                            sprite.draw(ui, pos);
                        }
                    }

                    let list_rect = egui::Rect::from_min_size(
                        popup_rect.min + egui::vec2(0.0, top_height),
                        egui::vec2(popup_size.x, list_height),
                    );

                    let mut selected = None;
                    ui.allocate_ui_at_rect(list_rect, |ui| {
                        egui::ScrollArea::vertical()
                            .auto_shrink([false; 2])
                            .max_height(list_height)
                            .show(ui, |ui| {
                                for (index, text) in items.iter() {
                                    let response = ui.add_sized(
                                        [popup_size.x, effective_row_height],
                                        egui::SelectableLabel::new(*index == selected_index, text),
                                    );
                                    if response.clicked() {
                                        selected = Some(*index);
                                    }
                                }
                            });
                    });

                    selected
                });

            popup_rect = popup_area.response.rect;
            pending_selection = popup_area.inner;
        }

        if popup_open && ui.input(|input| input.pointer.any_pressed()) {
            if let Some(pointer_pos) = ui.input(|input| input.pointer.interact_pos()) {
                if !clickable_rect.contains(pointer_pos) && !popup_rect.contains(pointer_pos) {
                    popup_open = false;
                }
            }
        }

        if let Some(new_selection) = pending_selection {
            if let Some((selected, _, _)) = bindings.get_combo(self.id) {
                if *selected != new_selection {
                    *selected = new_selection;
                    bindings.set_combo_changed(self.id, new_selection);
                }
            }
            popup_open = false;
        }

        ui.ctx()
            .data_mut(|data| data.insert_temp(popup_state_id, popup_open));
        bindings.set_response(self.id, response);
    }
}
