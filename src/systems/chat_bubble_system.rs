use bevy::{
    prelude::{
        Added, AssetServer, Assets, BuildChildren, Color, Commands, ComputedVisibility,
        DespawnRecursiveExt, Entity, EventReader, GlobalTransform, Handle, Image, Local, Query,
        RemovedComponents, Res, ResMut, Time, Transform, Vec2, Vec3, Visibility, With, Without,
    },
    render::{
        render_resource::{Extent3d, TextureDimension, TextureFormat},
        texture::ImageSampler,
        view::NoFrustumCulling,
    },
    utils::HashMap,
    window::PrimaryWindow,
};
#[cfg(not(windows))]
use bevy_egui::egui;
use bevy_egui::{EguiContexts, EguiManagedTextures};
use rose_game_common::components::Equipment;
#[cfg(not(windows))]
use std::{num::NonZeroU32, sync::Arc};

use crate::{
    components::{ClientEntity, ClientEntityType, ModelHeight, Vehicle},
    events::{LoadZoneEvent, WorldChatBubbleEvent},
    model_loader::ModelLoader,
    render::WorldUiRect,
};

const CHAT_BUBBLE_WIDTH: f32 = 179.0;
const CHAT_BUBBLE_WIDTH_PX: u32 = 179;
const CHAT_BUBBLE_HEIGHT_BIG: f32 = 50.0;
const CHAT_BUBBLE_HEIGHT_BIG_PX: u32 = 50;
const CHAT_BUBBLE_HEIGHT_SMALL: f32 = 30.0;
const CHAT_BUBBLE_HEIGHT_SMALL_PX: u32 = 30;
const CHAT_BUBBLE_TEXT_PADDING_X: i32 = 4;
const CHAT_BUBBLE_TEXT_PADDING_Y: i32 = 4;
const CHAT_BUBBLE_TEXT_MAX_ROWS: usize = 2;
const CHAT_BUBBLE_TEXT_SPLIT_LENGTH: usize = 28;
const CHAT_BUBBLE_LINE_SPACING: i32 = 2;
const CHAT_BUBBLE_FONT_POINT_SIZE: i32 = 11;
const CHAT_BUBBLE_FONT_NAME: &str = "Verdana";
const CHAT_BUBBLE_LIFETIME_SECONDS: f32 = 4.5;
const CHAT_BUBBLE_SINGLE_OFFSET_Y: f32 = 98.0;
const CHAT_BUBBLE_DOUBLE_OFFSET_Y: f32 = 118.0;
const CHAT_BUBBLE_ORDER_BACKGROUND: u8 = 10;
const CHAT_BUBBLE_ORDER_TEXT: u8 = 11;

#[derive(bevy::prelude::Component)]
pub struct ChatBubble;

#[derive(bevy::prelude::Component)]
pub struct ChatBubbleOwner(pub Entity);

#[derive(bevy::prelude::Component)]
pub struct ChatBubbleEntity(pub Entity);

#[derive(bevy::prelude::Component)]
pub struct ChatBubbleLifetime {
    pub remaining: f32,
}

#[derive(Clone)]
pub struct ChatBubbleAssets {
    pub single_line: Handle<Image>,
    pub multi_line: Handle<Image>,
}

#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq)]
enum ChatBubbleStyle {
    Default,
    Monster,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum ChatBubbleBackground {
    SingleLine,
    MultiLine,
}

#[derive(Debug, Hash, PartialEq, Eq, Clone)]
struct ChatBubbleCacheKey {
    text: String,
    pixels_per_point_bits: u32,
    style: ChatBubbleStyle,
    row_count: usize,
}

#[cfg(not(windows))]
#[derive(Clone)]
struct ChatBubblePendingData {
    cache_key: ChatBubbleCacheKey,
    galley: Arc<egui::Galley>,
}

#[derive(Clone)]
struct ChatBubbleData {
    row_count: usize,
    background_height: f32,
    rects: Vec<WorldUiRect>,
}

#[derive(Default)]
pub struct ChatBubbleCache {
    cache: HashMap<ChatBubbleCacheKey, ChatBubbleData>,
    #[cfg(not(windows))]
    pending: HashMap<Entity, ChatBubblePendingData>,
    pixels_per_point: f32,
    assets: Option<ChatBubbleAssets>,
}

fn bubble_background_height(row_count: usize) -> f32 {
    if row_count > 1 {
        CHAT_BUBBLE_HEIGHT_BIG
    } else {
        CHAT_BUBBLE_HEIGHT_SMALL
    }
}

fn bubble_background_height_px(row_count: usize) -> u32 {
    if row_count > 1 {
        CHAT_BUBBLE_HEIGHT_BIG_PX
    } else {
        CHAT_BUBBLE_HEIGHT_SMALL_PX
    }
}

fn bubble_background_offset(background_height: f32) -> Vec2 {
    debug_assert!(
        ((CHAT_BUBBLE_DOUBLE_OFFSET_Y - CHAT_BUBBLE_SINGLE_OFFSET_Y)
            - (CHAT_BUBBLE_HEIGHT_BIG - CHAT_BUBBLE_HEIGHT_SMALL))
            .abs()
            < f32::EPSILON
    );
    Vec2::new(
        -CHAT_BUBBLE_WIDTH / 2.0,
        CHAT_BUBBLE_SINGLE_OFFSET_Y + (background_height - CHAT_BUBBLE_HEIGHT_SMALL),
    )
}

fn bubble_background_kind(row_count: usize) -> ChatBubbleBackground {
    if row_count > 1 {
        ChatBubbleBackground::MultiLine
    } else {
        ChatBubbleBackground::SingleLine
    }
}

fn chat_bubble_style(client_entity: Option<&ClientEntity>) -> ChatBubbleStyle {
    if client_entity
        .is_some_and(|client_entity| client_entity.entity_type == ClientEntityType::Monster)
    {
        ChatBubbleStyle::Monster
    } else {
        ChatBubbleStyle::Default
    }
}

fn chat_bubble_text_color(style: ChatBubbleStyle) -> Color {
    match style {
        ChatBubbleStyle::Default => Color::BLACK,
        ChatBubbleStyle::Monster => Color::rgb(1.0, 0.0, 0.0),
    }
}

fn bubble_line_y(line_index: usize, font_height: i32) -> i32 {
    CHAT_BUBBLE_TEXT_PADDING_Y + line_index as i32 * (font_height + CHAT_BUBBLE_LINE_SPACING)
}

fn split_classic_chat_bubble_text(text: &str) -> Vec<String> {
    fn split_line_chunks(source: &str) -> Vec<String> {
        if source.is_empty() {
            return vec![String::new()];
        }

        let mut chunks = Vec::new();
        let mut start = 0usize;
        let mut len = 0usize;
        for (index, _) in source.char_indices() {
            if len == CHAT_BUBBLE_TEXT_SPLIT_LENGTH {
                chunks.push(source[start..index].to_string());
                start = index;
                len = 0;
            }
            len += 1;
        }
        chunks.push(source[start..].to_string());
        chunks
    }

    let mut lines = Vec::with_capacity(CHAT_BUBBLE_TEXT_MAX_ROWS);
    for source_line in text.replace("\r\n", "\n").split('\n') {
        for chunk in split_line_chunks(source_line) {
            if lines.len() == CHAT_BUBBLE_TEXT_MAX_ROWS {
                return lines;
            }
            lines.push(chunk);
        }
    }
    lines
}

#[cfg(not(windows))]
fn rendered_chat_bubble_text(lines: &[String]) -> String {
    lines.join("\n")
}

fn reset_chat_bubble_lifetime(lifetime: &mut ChatBubbleLifetime) {
    lifetime.remaining = CHAT_BUBBLE_LIFETIME_SECONDS;
}

fn advance_chat_bubble_lifetime(lifetime: &mut ChatBubbleLifetime, delta_seconds: f32) -> bool {
    lifetime.remaining -= delta_seconds;
    lifetime.remaining <= 0.0
}

fn bubble_anchor_height(
    model_loader: &ModelLoader,
    model_height: Option<&ModelHeight>,
    equipment: Option<&Equipment>,
    vehicle: bool,
) -> f32 {
    let base_height = model_height.map_or(1.8, |model_height| model_height.height);

    if vehicle {
        base_height
            + equipment.map_or(0.0, |equipment| {
                model_loader.get_vehicle_driver_seat_height(equipment)
            })
    } else {
        base_height
    }
}

#[cfg(windows)]
fn current_chat_bubble_scale_value(_egui_context: &mut EguiContexts) -> f32 {
    classic_windows_text::chat_bubble_font_height() as f32
}

#[cfg(not(windows))]
fn current_chat_bubble_scale_value(egui_context: &mut EguiContexts) -> f32 {
    egui_context.ctx_mut().pixels_per_point()
}

fn ensure_chat_bubble_assets(
    asset_server: &AssetServer,
    assets: &mut Option<ChatBubbleAssets>,
) -> ChatBubbleAssets {
    assets
        .get_or_insert_with(|| ChatBubbleAssets {
            single_line: asset_server.load("3DDATA/CONTROL/RES/CHATBOX01.TGA"),
            multi_line: asset_server.load("3DDATA/CONTROL/RES/CHATBOX02.TGA"),
        })
        .clone()
}

fn create_alpha_mask_rgba(alpha_data: &[u8], width: usize, height: usize) -> Vec<u8> {
    let mut rgba_data = vec![0; width * height * 4];
    for y in 0..height {
        for x in 0..width {
            let alpha = alpha_data[x + y * width];
            let pixel_offset = (x + y * width) * 4;
            if alpha > 0 {
                rgba_data[pixel_offset] = 255;
                rgba_data[pixel_offset + 1] = 255;
                rgba_data[pixel_offset + 2] = 255;
                rgba_data[pixel_offset + 3] = alpha;
            }
        }
    }
    rgba_data
}

fn add_chat_bubble_mask_image(
    images: &mut Assets<Image>,
    alpha_data: &[u8],
    width: u32,
    height: u32,
) -> Handle<Image> {
    let mut image = Image::new(
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        create_alpha_mask_rgba(alpha_data, width as usize, height as usize),
        TextureFormat::Rgba8Unorm,
    );
    image.sampler_descriptor = ImageSampler::Descriptor(ImageSampler::nearest_descriptor());
    images.add(image)
}

#[cfg(windows)]
fn create_chat_bubble_data_windows(
    images: &mut Assets<Image>,
    cache_key: &ChatBubbleCacheKey,
    lines: &[String],
) -> Option<ChatBubbleData> {
    let row_count = lines.len();
    if row_count == 0 {
        return None;
    }

    let background_height = bubble_background_height(row_count);
    let background_height_px = bubble_background_height_px(row_count);
    let alpha_data = classic_windows_text::render_chat_bubble_alpha(lines, background_height_px)?;
    let image = add_chat_bubble_mask_image(
        images,
        &alpha_data,
        CHAT_BUBBLE_WIDTH_PX,
        background_height_px,
    );
    let background_offset = bubble_background_offset(background_height);

    Some(ChatBubbleData {
        row_count,
        background_height,
        rects: vec![WorldUiRect {
            image,
            screen_offset: background_offset,
            screen_size: Vec2::new(CHAT_BUBBLE_WIDTH, background_height),
            uv_min: Vec2::ZERO,
            uv_max: Vec2::ONE,
            color: chat_bubble_text_color(cache_key.style),
            order: CHAT_BUBBLE_ORDER_TEXT,
        }],
    })
}

#[cfg(not(windows))]
fn create_chat_bubble_galley(
    egui_context: &egui::Context,
    text: &str,
    pixels_per_point: f32,
) -> Arc<egui::Galley> {
    let mut layout_job = egui::epaint::text::LayoutJob::single_section(
        text.to_string(),
        egui::TextFormat::simple(
            egui::FontId::proportional(CHAT_BUBBLE_FONT_POINT_SIZE as f32),
            egui::Color32::WHITE,
        ),
    );
    layout_job.wrap.max_width = f32::INFINITY / pixels_per_point.max(1.0);
    egui_context.fonts(|fonts| fonts.layout_job(layout_job))
}

#[cfg(not(windows))]
fn next_power_of_two(value: u32) -> u32 {
    NonZeroU32::new(value.max(1))
        .map(NonZeroU32::get)
        .unwrap()
        .next_power_of_two()
}

#[cfg(not(windows))]
fn create_chat_bubble_data_nonwindows(
    window_entity: Entity,
    egui_managed_textures: &EguiManagedTextures,
    images: &mut Assets<Image>,
    pending_data: ChatBubblePendingData,
) -> Option<ChatBubbleData> {
    let row_count = pending_data.cache_key.row_count;
    let pixels_per_point = f32::from_bits(pending_data.cache_key.pixels_per_point_bits);
    let rows: Vec<_> = pending_data.galley.rows.iter().take(row_count).collect();
    let row_count = rows.len();
    if row_count == 0 {
        return None;
    }

    let logical_min = rows
        .iter()
        .flat_map(|row| row.glyphs.iter().map(|glyph| glyph.logical_rect().min))
        .fold(egui::pos2(f32::MAX, f32::MAX), |min, glyph_min| {
            egui::pos2(min.x.min(glyph_min.x), min.y.min(glyph_min.y))
        });
    let logical_max = rows
        .iter()
        .flat_map(|row| row.glyphs.iter().map(|glyph| glyph.logical_rect().max))
        .fold(egui::pos2(f32::MIN, f32::MIN), |max, glyph_max| {
            egui::pos2(max.x.max(glyph_max.x), max.y.max(glyph_max.y))
        });

    let text_size = Vec2::new(
        (logical_max.x - logical_min.x).max(1.0) * pixels_per_point,
        (logical_max.y - logical_min.y).max(1.0) * pixels_per_point,
    )
    .ceil();
    let texture_width = next_power_of_two(text_size.x.max(1.0) as u32);
    let texture_height = next_power_of_two(text_size.y.max(1.0) as u32);
    let mut fill_alpha = vec![0; (texture_width * texture_height) as usize];
    let dst_stride = texture_width as usize;

    for row in rows.iter() {
        let font_texture_id = match row.visuals.mesh.texture_id {
            egui::TextureId::Managed(id) => id,
            egui::TextureId::User(_) => unreachable!(),
        };
        let Some(managed_texture) = egui_managed_textures
            .0
            .get(&(window_entity, font_texture_id))
        else {
            return None;
        };

        let font_texture = &managed_texture.color_image;
        let src_stride = font_texture.width();

        unsafe {
            let src = font_texture.pixels.as_ptr();
            let dst = fill_alpha.as_mut_ptr();

            for glyph in row.glyphs.iter() {
                let uv_min = glyph.uv_rect.min;
                let uv_max = glyph.uv_rect.max;
                let glyph_min = Vec2::new(
                    (glyph.pos.x + glyph.uv_rect.offset.x - logical_min.x) * pixels_per_point,
                    (glyph.pos.y + glyph.uv_rect.offset.y - logical_min.y) * pixels_per_point,
                );
                let dst_x = glyph_min.x.round().max(0.0) as usize;
                let mut dst_y = glyph_min.y.round().max(0.0) as usize;

                for uv_y in uv_min[1]..uv_max[1] {
                    let mut src_row = src.add(uv_y as usize * src_stride + uv_min[0] as usize);
                    let mut dst_row = dst.add(dst_y * dst_stride + dst_x);

                    for _ in uv_min[0]..uv_max[0] {
                        let pixel = (*src_row).to_array();
                        *dst_row = (*dst_row).max(pixel[3]);
                        src_row = src_row.add(1);
                        dst_row = dst_row.add(1);
                    }

                    dst_y += 1;
                }
            }
        }
    }

    let image = add_chat_bubble_mask_image(images, &fill_alpha, texture_width, texture_height);
    let background_height = bubble_background_height(row_count);
    let background_offset = bubble_background_offset(background_height);

    Some(ChatBubbleData {
        row_count,
        background_height,
        rects: vec![WorldUiRect {
            image,
            screen_offset: Vec2::new(
                background_offset.x + CHAT_BUBBLE_TEXT_PADDING_X as f32,
                background_offset.y + CHAT_BUBBLE_TEXT_PADDING_Y as f32,
            ),
            screen_size: text_size,
            uv_min: Vec2::ZERO,
            uv_max: Vec2::ONE,
            color: chat_bubble_text_color(pending_data.cache_key.style),
            order: CHAT_BUBBLE_ORDER_TEXT,
        }],
    })
}

fn despawn_chat_bubble(
    commands: &mut Commands,
    owner_entity: Entity,
    bubble_entity: Entity,
    query_bubble_entity: &Query<&ChatBubbleEntity>,
) {
    if query_bubble_entity
        .get(owner_entity)
        .map_or(false, |chat_bubble_entity| {
            chat_bubble_entity.0 == bubble_entity
        })
    {
        commands.entity(owner_entity).remove::<ChatBubbleEntity>();
    }
    commands.entity(bubble_entity).despawn_recursive();
}

pub fn chat_bubble_system(
    mut commands: Commands,
    mut chat_bubble_cache: Local<ChatBubbleCache>,
    mut world_chat_bubble_events: EventReader<WorldChatBubbleEvent>,
    mut load_zone_events: EventReader<LoadZoneEvent>,
    mut egui_context: EguiContexts,
    #[cfg_attr(windows, allow(unused_variables))] egui_managed_textures: Res<EguiManagedTextures>,
    asset_server: Res<AssetServer>,
    mut images: ResMut<Assets<Image>>,
    model_loader: Res<ModelLoader>,
    time: Res<Time>,
    #[cfg_attr(windows, allow(unused_variables))] query_window: Query<Entity, With<PrimaryWindow>>,
    query_bubbles: Query<(Entity, &ChatBubbleOwner)>,
    mut query_lifetimes: Query<
        (Entity, &ChatBubbleOwner, &mut ChatBubbleLifetime),
        With<ChatBubble>,
    >,
    query_bubble_entity: Query<&ChatBubbleEntity>,
    query_owner: Query<(
        Option<&ChatBubbleEntity>,
        Option<&ModelHeight>,
        Option<&Equipment>,
        Option<&Vehicle>,
        Option<&ClientEntity>,
    )>,
) {
    #[cfg(not(windows))]
    let Ok(window_entity) = query_window.get_single() else {
        return;
    };

    let scale_value = current_chat_bubble_scale_value(&mut egui_context);
    let zone_changed = load_zone_events.iter().last().is_some();
    let dpi_changed = chat_bubble_cache.pixels_per_point != 0.0
        && (chat_bubble_cache.pixels_per_point - scale_value).abs() > f32::EPSILON;

    if zone_changed || dpi_changed {
        for (bubble_entity, bubble_owner) in query_bubbles.iter() {
            despawn_chat_bubble(
                &mut commands,
                bubble_owner.0,
                bubble_entity,
                &query_bubble_entity,
            );
        }

        chat_bubble_cache.cache.clear();
        #[cfg(not(windows))]
        chat_bubble_cache.pending.clear();
        chat_bubble_cache.pixels_per_point = scale_value;

        for _ in world_chat_bubble_events.iter() {}
        return;
    } else if chat_bubble_cache.pixels_per_point == 0.0 {
        chat_bubble_cache.pixels_per_point = scale_value;
    }

    let bubble_assets = ensure_chat_bubble_assets(&asset_server, &mut chat_bubble_cache.assets);
    let expired_bubbles: Vec<_> = query_lifetimes
        .iter_mut()
        .filter_map(|(bubble_entity, bubble_owner, mut lifetime)| {
            if advance_chat_bubble_lifetime(&mut lifetime, time.delta_seconds()) {
                Some((bubble_entity, bubble_owner.0))
            } else {
                None
            }
        })
        .collect();
    for (bubble_entity, bubble_owner) in expired_bubbles {
        despawn_chat_bubble(
            &mut commands,
            bubble_owner,
            bubble_entity,
            &query_bubble_entity,
        );
    }

    #[cfg(not(windows))]
    {
        let pending_entities: Vec<_> = chat_bubble_cache.pending.keys().copied().collect();
        for owner_entity in pending_entities {
            let Some(pending_data) = chat_bubble_cache.pending.remove(&owner_entity) else {
                continue;
            };
            let cache_key = pending_data.cache_key.clone();
            let pending_data_clone = pending_data.clone();

            let Some(chat_bubble_data) = create_chat_bubble_data_nonwindows(
                window_entity,
                &egui_managed_textures,
                &mut images,
                pending_data,
            ) else {
                chat_bubble_cache
                    .pending
                    .insert(owner_entity, pending_data_clone);
                continue;
            };

            chat_bubble_cache
                .cache
                .insert(cache_key, chat_bubble_data.clone());

            let Ok((existing_bubble, model_height, equipment, vehicle, _)) =
                query_owner.get(owner_entity)
            else {
                continue;
            };
            let anchor_height =
                bubble_anchor_height(&model_loader, model_height, equipment, vehicle.is_some());
            spawn_chat_bubble(
                &mut commands,
                owner_entity,
                existing_bubble.map(|entity| entity.0),
                &query_bubble_entity,
                &bubble_assets,
                &chat_bubble_data,
                anchor_height,
            );
        }
    }

    for event in world_chat_bubble_events.iter() {
        let text = event.text.trim();
        if text.is_empty() {
            continue;
        }

        let Ok((existing_bubble, model_height, equipment, vehicle, client_entity)) =
            query_owner.get(event.entity)
        else {
            continue;
        };

        let style = chat_bubble_style(client_entity);
        let lines = split_classic_chat_bubble_text(text);
        let row_count = lines.len();
        if row_count == 0 {
            continue;
        }

        let cache_key = ChatBubbleCacheKey {
            text: text.to_string(),
            pixels_per_point_bits: scale_value.to_bits(),
            style,
            row_count,
        };
        let anchor_height =
            bubble_anchor_height(&model_loader, model_height, equipment, vehicle.is_some());

        if let Some(chat_bubble_data) = chat_bubble_cache.cache.get(&cache_key).cloned() {
            spawn_chat_bubble(
                &mut commands,
                event.entity,
                existing_bubble.map(|entity| entity.0),
                &query_bubble_entity,
                &bubble_assets,
                &chat_bubble_data,
                anchor_height,
            );
            continue;
        }

        #[cfg(windows)]
        {
            let Some(chat_bubble_data) =
                create_chat_bubble_data_windows(&mut images, &cache_key, &lines)
            else {
                continue;
            };

            chat_bubble_cache
                .cache
                .insert(cache_key, chat_bubble_data.clone());

            spawn_chat_bubble(
                &mut commands,
                event.entity,
                existing_bubble.map(|entity| entity.0),
                &query_bubble_entity,
                &bubble_assets,
                &chat_bubble_data,
                anchor_height,
            );
        }

        #[cfg(not(windows))]
        {
            let rendered_text = rendered_chat_bubble_text(&lines);
            let galley =
                create_chat_bubble_galley(egui_context.ctx_mut(), &rendered_text, scale_value);
            chat_bubble_cache
                .pending
                .insert(event.entity, ChatBubblePendingData { cache_key, galley });
        }
    }
}

fn spawn_chat_bubble(
    commands: &mut Commands,
    owner_entity: Entity,
    existing_bubble: Option<Entity>,
    query_bubble_entity: &Query<&ChatBubbleEntity>,
    bubble_assets: &ChatBubbleAssets,
    chat_bubble_data: &ChatBubbleData,
    anchor_height: f32,
) {
    if let Some(existing_bubble) = existing_bubble {
        despawn_chat_bubble(commands, owner_entity, existing_bubble, query_bubble_entity);
    }

    let mut lifetime = ChatBubbleLifetime { remaining: 0.0 };
    reset_chat_bubble_lifetime(&mut lifetime);

    let bubble_entity = commands
        .spawn((
            ChatBubble,
            ChatBubbleOwner(owner_entity),
            lifetime,
            Visibility::Inherited,
            ComputedVisibility::default(),
            Transform::from_translation(Vec3::new(0.0, anchor_height, 0.0)),
            GlobalTransform::default(),
            NoFrustumCulling,
        ))
        .id();

    let background_offset = bubble_background_offset(chat_bubble_data.background_height);
    let background_image =
        if bubble_background_kind(chat_bubble_data.row_count) == ChatBubbleBackground::MultiLine {
            bubble_assets.multi_line.clone_weak()
        } else {
            bubble_assets.single_line.clone_weak()
        };

    commands.entity(bubble_entity).with_children(|parent| {
        parent.spawn((
            WorldUiRect {
                image: background_image,
                screen_offset: background_offset,
                screen_size: Vec2::new(CHAT_BUBBLE_WIDTH, chat_bubble_data.background_height),
                uv_min: Vec2::ZERO,
                uv_max: Vec2::ONE,
                color: Color::WHITE,
                order: CHAT_BUBBLE_ORDER_BACKGROUND,
            },
            Transform::default(),
            GlobalTransform::default(),
            Visibility::Inherited,
            ComputedVisibility::default(),
            NoFrustumCulling,
        ));

        for rect in chat_bubble_data.rects.iter() {
            parent.spawn((
                rect.clone(),
                Transform::default(),
                GlobalTransform::default(),
                Visibility::Inherited,
                ComputedVisibility::default(),
                NoFrustumCulling,
            ));
        }
    });

    commands
        .entity(owner_entity)
        .insert(ChatBubbleEntity(bubble_entity))
        .add_child(bubble_entity);
}

pub fn chat_bubble_vehicle_height_system(
    query_new_drivers: Query<(&ChatBubbleEntity, &Equipment, Option<&ModelHeight>), Added<Vehicle>>,
    query_stopped_drivers: Query<(&ChatBubbleEntity, &ModelHeight), Without<Vehicle>>,
    mut removed_vehicles: RemovedComponents<Vehicle>,
    mut query_chat_bubble_transform: Query<&mut Transform, (With<ChatBubble>, Without<Vehicle>)>,
    model_loader: Res<ModelLoader>,
) {
    for (chat_bubble_entity, equipment, model_height) in query_new_drivers.iter() {
        let driver_seat_height = model_loader.get_vehicle_driver_seat_height(equipment);
        let character_height = model_height.map_or(1.8, |height| height.height);
        if let Ok(mut transform) = query_chat_bubble_transform.get_mut(chat_bubble_entity.0) {
            transform.translation.y = driver_seat_height + character_height;
        }
    }

    for entity in removed_vehicles.iter() {
        if let Ok((chat_bubble_entity, model_height)) = query_stopped_drivers.get(entity) {
            if let Ok(mut transform) = query_chat_bubble_transform.get_mut(chat_bubble_entity.0) {
                transform.translation.y = model_height.height;
            }
        }
    }
}

#[cfg(windows)]
mod classic_windows_text {
    use super::{
        bubble_line_y, CHAT_BUBBLE_FONT_NAME, CHAT_BUBBLE_FONT_POINT_SIZE,
        CHAT_BUBBLE_TEXT_PADDING_X, CHAT_BUBBLE_WIDTH_PX,
    };
    use std::{ffi::c_void, mem::zeroed, ptr::null_mut};
    use windows_sys::Win32::Graphics::Gdi::{
        CreateCompatibleDC, CreateDIBSection, CreateFontW, DeleteDC, DeleteObject, GetDC,
        GetDeviceCaps, GetTextMetricsW, ReleaseDC, SelectObject, SetBkMode, SetTextColor, TextOutW,
        BITMAPINFO, BITMAPINFOHEADER, BI_RGB, CLIP_DEFAULT_PRECIS, DEFAULT_CHARSET, DIB_RGB_COLORS,
        FF_MODERN, FIXED_PITCH, FW_NORMAL, HBITMAP, HFONT, HGDIOBJ, LOGPIXELSY, PROOF_QUALITY,
        TEXTMETRICW, TRANSPARENT,
    };

    fn mul_div(value: i32, numerator: i32, denominator: i32) -> i32 {
        (((value as i64 * numerator as i64) + (denominator as i64 / 2)) / denominator as i64) as i32
    }

    pub(super) fn chat_bubble_font_height() -> i32 {
        unsafe {
            let screen_dc = GetDC(0);
            if screen_dc == 0 {
                return CHAT_BUBBLE_FONT_POINT_SIZE.max(1);
            }

            let logical_pixels_y = GetDeviceCaps(screen_dc, LOGPIXELSY as i32).max(96);
            ReleaseDC(0, screen_dc);
            mul_div(CHAT_BUBBLE_FONT_POINT_SIZE, logical_pixels_y, 72).max(1)
        }
    }

    unsafe fn create_chat_bubble_font(font_height: i32) -> HFONT {
        let mut font_name: Vec<u16> = CHAT_BUBBLE_FONT_NAME.encode_utf16().collect();
        font_name.push(0);

        CreateFontW(
            font_height,
            0,
            0,
            0,
            FW_NORMAL as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET as u32,
            0,
            CLIP_DEFAULT_PRECIS as u32,
            PROOF_QUALITY as u32,
            (FIXED_PITCH | FF_MODERN) as u32,
            font_name.as_ptr(),
        )
    }

    pub(super) fn render_chat_bubble_alpha(
        lines: &[String],
        bubble_height: u32,
    ) -> Option<Vec<u8>> {
        unsafe {
            let screen_dc = GetDC(0);
            if screen_dc == 0 {
                return None;
            }

            let memory_dc = CreateCompatibleDC(screen_dc);
            if memory_dc == 0 {
                ReleaseDC(0, screen_dc);
                return None;
            }

            let mut old_bitmap: HGDIOBJ = 0;
            let mut old_font: HGDIOBJ = 0;
            let mut dib: HBITMAP = 0;
            let mut font: HFONT = 0;

            let result = (|| {
                let mut bitmap_info: BITMAPINFO = zeroed();
                bitmap_info.bmiHeader = BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: CHAT_BUBBLE_WIDTH_PX as i32,
                    biHeight: -(bubble_height as i32),
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB as u32,
                    ..zeroed()
                };

                let mut pixel_data: *mut c_void = null_mut();
                dib = CreateDIBSection(
                    screen_dc,
                    &bitmap_info,
                    DIB_RGB_COLORS,
                    &mut pixel_data,
                    0,
                    0,
                );
                if dib == 0 || pixel_data.is_null() {
                    return None;
                }

                old_bitmap = SelectObject(memory_dc, dib as HGDIOBJ);
                font = create_chat_bubble_font(chat_bubble_font_height());
                if font == 0 {
                    return None;
                }
                old_font = SelectObject(memory_dc, font as HGDIOBJ);

                std::ptr::write_bytes(
                    pixel_data,
                    0,
                    CHAT_BUBBLE_WIDTH_PX as usize * bubble_height as usize * 4,
                );
                SetBkMode(memory_dc, TRANSPARENT as i32);
                SetTextColor(memory_dc, 0x00FF_FFFF);

                let mut metrics: TEXTMETRICW = zeroed();
                let font_height = if GetTextMetricsW(memory_dc, &mut metrics) != 0 {
                    metrics.tmHeight
                } else {
                    chat_bubble_font_height()
                };

                for (line_index, line) in lines.iter().enumerate() {
                    let utf16: Vec<u16> = line.encode_utf16().collect();
                    if utf16.is_empty() {
                        continue;
                    }

                    TextOutW(
                        memory_dc,
                        CHAT_BUBBLE_TEXT_PADDING_X,
                        bubble_line_y(line_index, font_height),
                        utf16.as_ptr(),
                        utf16.len() as i32,
                    );
                }

                let pixel_count = CHAT_BUBBLE_WIDTH_PX as usize * bubble_height as usize;
                let bgra = std::slice::from_raw_parts(pixel_data as *const u8, pixel_count * 4);
                let mut alpha = vec![0u8; pixel_count];
                for pixel_index in 0..pixel_count {
                    let channel_index = pixel_index * 4;
                    let blue = bgra[channel_index];
                    let green = bgra[channel_index + 1];
                    let red = bgra[channel_index + 2];
                    alpha[pixel_index] = red.max(green).max(blue);
                }
                Some(alpha)
            })();

            if old_font != 0 {
                SelectObject(memory_dc, old_font);
            }
            if font != 0 {
                DeleteObject(font as HGDIOBJ);
            }
            if old_bitmap != 0 {
                SelectObject(memory_dc, old_bitmap);
            }
            if dib != 0 {
                DeleteObject(dib as HGDIOBJ);
            }
            DeleteDC(memory_dc);
            ReleaseDC(0, screen_dc);
            result
        }
    }
}

#[cfg(test)]
mod tests {
    use bevy::prelude::Color;

    use super::{
        advance_chat_bubble_lifetime, bubble_background_height, bubble_background_kind,
        bubble_background_offset, bubble_line_y, chat_bubble_text_color,
        reset_chat_bubble_lifetime, split_classic_chat_bubble_text, ChatBubbleBackground,
        ChatBubbleCacheKey, ChatBubbleLifetime, ChatBubbleStyle, CHAT_BUBBLE_DOUBLE_OFFSET_Y,
        CHAT_BUBBLE_SINGLE_OFFSET_Y, CHAT_BUBBLE_TEXT_MAX_ROWS,
    };

    #[test]
    fn classic_splitter_caps_bubbles_to_two_rows() {
        let lines = split_classic_chat_bubble_text(
            "ABCDEFGHIJKLMNOPQRSTUVWXYZ12abcdefghijklmnopqrstuvwxyz34EXTRA",
        );

        assert_eq!(lines.len(), CHAT_BUBBLE_TEXT_MAX_ROWS);
        assert_eq!(lines[0].chars().count(), 28);
        assert_eq!(lines[1].chars().count(), 28);
    }

    #[test]
    fn classic_splitter_preserves_newlines_before_cap() {
        let lines = split_classic_chat_bubble_text("TEST\nSECOND\nTHIRD");
        assert_eq!(lines, vec!["TEST".to_string(), "SECOND".to_string()]);
    }

    #[test]
    fn chat_bubble_cache_key_includes_style() {
        let default_key = ChatBubbleCacheKey {
            text: "Hello".to_string(),
            pixels_per_point_bits: 1.0f32.to_bits(),
            style: ChatBubbleStyle::Default,
            row_count: 1,
        };
        let monster_key = ChatBubbleCacheKey {
            style: ChatBubbleStyle::Monster,
            ..default_key.clone()
        };
        assert_ne!(default_key, monster_key);
    }

    #[test]
    fn chat_bubble_background_selection_matches_original_assets() {
        assert_eq!(bubble_background_kind(1), ChatBubbleBackground::SingleLine);
        assert_eq!(bubble_background_kind(2), ChatBubbleBackground::MultiLine);
    }

    #[test]
    fn chat_bubble_background_height_and_offset_match_classic_sizes() {
        assert_eq!(bubble_background_height(1), 30.0);
        assert_eq!(bubble_background_height(2), 50.0);
        assert_eq!(
            bubble_background_offset(30.0).y,
            CHAT_BUBBLE_SINGLE_OFFSET_Y
        );
        assert_eq!(
            bubble_background_offset(50.0).y,
            CHAT_BUBBLE_DOUBLE_OFFSET_Y
        );
    }

    #[test]
    fn bubble_line_positions_use_classic_padding_and_spacing() {
        assert_eq!(bubble_line_y(0, 13), 4);
        assert_eq!(bubble_line_y(1, 13), 19);
    }

    #[test]
    fn monster_bubbles_only_change_text_color() {
        assert_eq!(
            chat_bubble_text_color(ChatBubbleStyle::Monster).as_rgba_f32(),
            Color::rgb(1.0, 0.0, 0.0).as_rgba_f32()
        );
        assert_eq!(
            chat_bubble_text_color(ChatBubbleStyle::Default).as_rgba_f32(),
            Color::BLACK.as_rgba_f32()
        );
    }

    #[test]
    fn chat_bubble_lifetime_resets_and_expires() {
        let mut lifetime = ChatBubbleLifetime { remaining: 0.0 };
        reset_chat_bubble_lifetime(&mut lifetime);
        assert!(!advance_chat_bubble_lifetime(&mut lifetime, 4.0));

        reset_chat_bubble_lifetime(&mut lifetime);
        assert!(!advance_chat_bubble_lifetime(&mut lifetime, 4.0));
        assert!(advance_chat_bubble_lifetime(&mut lifetime, 0.6));
    }
}
