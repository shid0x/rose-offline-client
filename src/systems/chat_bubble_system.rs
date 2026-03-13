use std::{num::NonZeroU32, sync::Arc};

use arrayvec::ArrayVec;
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
use bevy_egui::{egui, EguiContexts, EguiManagedTextures};
use rose_game_common::components::Equipment;

use crate::{
    components::{ModelHeight, Vehicle},
    events::{LoadZoneEvent, WorldChatBubbleEvent},
    model_loader::ModelLoader,
    render::WorldUiRect,
};

const CHAT_BUBBLE_WIDTH: f32 = 179.0;
const CHAT_BUBBLE_HEIGHT_BIG: f32 = 50.0;
const CHAT_BUBBLE_HEIGHT_SMALL: f32 = 30.0;
const CHAT_BUBBLE_TEXT_PADDING_X: f32 = 10.0;
const CHAT_BUBBLE_TEXT_MAX_ROWS: usize = 2;
const CHAT_BUBBLE_TEXT_MAX_WIDTH: f32 = CHAT_BUBBLE_WIDTH - CHAT_BUBBLE_TEXT_PADDING_X * 2.0;
const CHAT_BUBBLE_LINE_SPACING: f32 = 2.0;
const CHAT_BUBBLE_FONT_SIZE: f32 = 18.0;
const CHAT_BUBBLE_TEXT_RENDER_SCALE: f32 = 2.0;
const CHAT_BUBBLE_LIFETIME_SECONDS: f32 = 4.5;
const CHAT_BUBBLE_SINGLE_OFFSET_Y: f32 = 98.0;
const CHAT_BUBBLE_DOUBLE_OFFSET_Y: f32 = 118.0;
const CHAT_BUBBLE_ORDER_BACKGROUND: u8 = 10;
const CHAT_BUBBLE_ORDER_TEXT: u8 = 11;
const CHAT_BUBBLE_TEXT_TEXTURE_PADDING: usize = 2;

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
    pub small: Handle<Image>,
    pub big: Handle<Image>,
}

#[derive(Hash, PartialEq, Eq, Clone)]
struct ChatBubbleCacheKey {
    text: String,
    pixels_per_point_bits: u32,
    row_count: usize,
}

#[derive(Clone)]
struct ChatBubblePendingData {
    cache_key: ChatBubbleCacheKey,
    galley: Arc<egui::Galley>,
}

#[derive(Clone)]
struct ChatBubbleData {
    row_count: usize,
    rows: ArrayVec<WorldUiRect, CHAT_BUBBLE_TEXT_MAX_ROWS>,
}

#[derive(Default)]
pub struct ChatBubbleCache {
    cache: HashMap<ChatBubbleCacheKey, ChatBubbleData>,
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

fn bubble_background_offset(row_count: usize) -> Vec2 {
    let offset_y = if row_count > 1 {
        CHAT_BUBBLE_DOUBLE_OFFSET_Y
    } else {
        CHAT_BUBBLE_SINGLE_OFFSET_Y
    };

    Vec2::new(-CHAT_BUBBLE_WIDTH / 2.0, offset_y)
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

fn create_chat_bubble_galley(
    egui_context: &egui::Context,
    text: &str,
    pixels_per_point: f32,
) -> Arc<egui::Galley> {
    let mut layout_job = egui::epaint::text::LayoutJob::single_section(
        text.to_string(),
        egui::TextFormat::simple(
            egui::FontId::proportional(CHAT_BUBBLE_FONT_SIZE * CHAT_BUBBLE_TEXT_RENDER_SCALE),
            egui::Color32::BLACK,
        ),
    );
    layout_job.wrap.max_width =
        CHAT_BUBBLE_TEXT_MAX_WIDTH * CHAT_BUBBLE_TEXT_RENDER_SCALE / pixels_per_point.max(1.0);

    egui_context.fonts(|fonts| fonts.layout_job(layout_job))
}

fn retained_chat_bubble_row_count(galley: &egui::Galley) -> usize {
    galley.rows.len().min(CHAT_BUBBLE_TEXT_MAX_ROWS)
}

fn reset_chat_bubble_lifetime(lifetime: &mut ChatBubbleLifetime) {
    lifetime.remaining = CHAT_BUBBLE_LIFETIME_SECONDS;
}

fn advance_chat_bubble_lifetime(lifetime: &mut ChatBubbleLifetime, delta_seconds: f32) -> bool {
    lifetime.remaining -= delta_seconds;
    lifetime.remaining <= 0.0
}

fn next_power_of_two(value: u32) -> u32 {
    NonZeroU32::new(value.max(1))
        .map(NonZeroU32::get)
        .unwrap()
        .next_power_of_two()
}

fn ensure_chat_bubble_assets(
    asset_server: &AssetServer,
    assets: &mut Option<ChatBubbleAssets>,
) -> ChatBubbleAssets {
    assets
        .get_or_insert_with(|| ChatBubbleAssets {
            small: asset_server.load("3DDATA/CONTROL/RES/CHATBOX01.TGA"),
            big: asset_server.load("3DDATA/CONTROL/RES/CHATBOX02.TGA"),
        })
        .clone()
}

fn create_chat_bubble_data(
    window_entity: Entity,
    egui_managed_textures: &EguiManagedTextures,
    images: &mut Assets<Image>,
    pending_data: ChatBubblePendingData,
) -> Option<ChatBubbleData> {
    let row_count = pending_data.cache_key.row_count;
    let pixels_per_point = f32::from_bits(pending_data.cache_key.pixels_per_point_bits);
    let background_height = bubble_background_height(row_count);
    let mut rows: ArrayVec<WorldUiRect, CHAT_BUBBLE_TEXT_MAX_ROWS> = ArrayVec::new();
    let background_offset = bubble_background_offset(row_count);
    let mut row_images = Vec::with_capacity(row_count);

    for row in pending_data.galley.rows.iter().take(row_count) {
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

        let mut row_min = Vec2::new(f32::MAX, f32::MAX);
        let mut row_max = Vec2::ZERO;
        for glyph in row.glyphs.iter() {
            let glyph_size = Vec2::new(
                glyph.uv_rect.max[0] as f32 - glyph.uv_rect.min[0] as f32,
                glyph.uv_rect.max[1] as f32 - glyph.uv_rect.min[1] as f32,
            );
            let glyph_min = Vec2::new(
                glyph.pos.x + glyph.uv_rect.offset.x,
                glyph.pos.y + glyph.uv_rect.offset.y,
            ) * pixels_per_point;
            let glyph_max = glyph_min + glyph_size;

            row_min = row_min.min(glyph_min);
            row_max = row_max.max(glyph_max);
        }

        if !row_min.is_finite() || row.glyphs.is_empty() {
            continue;
        }

        let text_size = (row_max - row_min).ceil().max(Vec2::splat(1.0));
        let row_size = text_size + Vec2::splat((CHAT_BUBBLE_TEXT_TEXTURE_PADDING * 2) as f32);
        let target_texture_width = next_power_of_two(row_size.x as u32);
        let target_texture_height = next_power_of_two(row_size.y as u32);
        let mut alpha_data = vec![0; (target_texture_width * target_texture_height) as usize];
        let font_texture = &managed_texture.color_image;
        let src_stride = font_texture.width();
        let dst_stride = target_texture_width as usize;

        unsafe {
            let src = font_texture.pixels.as_ptr();
            let dst = alpha_data.as_mut_ptr();

            for glyph in row.glyphs.iter() {
                let uv_min = glyph.uv_rect.min;
                let uv_max = glyph.uv_rect.max;

                let glyph_min = Vec2::new(
                    (glyph.pos.x + glyph.uv_rect.offset.x) * pixels_per_point,
                    (glyph.pos.y + glyph.uv_rect.offset.y) * pixels_per_point,
                );
                let dst_x = (glyph_min.x - row_min.x).round().max(0.0) as usize
                    + CHAT_BUBBLE_TEXT_TEXTURE_PADDING;
                let mut dst_y = (glyph_min.y - row_min.y).round().max(0.0) as usize
                    + CHAT_BUBBLE_TEXT_TEXTURE_PADDING;

                for uv_y in uv_min[1]..uv_max[1] {
                    let mut src_row = src.add(uv_y as usize * src_stride + uv_min[0] as usize);
                    let mut dst_row = dst.add(dst_y * dst_stride + dst_x);

                    for _ in uv_min[0]..uv_max[0] {
                        let pixel = (*src_row).to_array();
                        *dst_row = pixel[3];

                        src_row = src_row.add(1);
                        dst_row = dst_row.add(1);
                    }

                    dst_y += 1;
                }
            }
        }

        let mut rgba_data = vec![0; (target_texture_width * target_texture_height * 4) as usize];
        for y in 0..target_texture_height as usize {
            for x in 0..target_texture_width as usize {
                let alpha = alpha_data[x + y * dst_stride];
                let shadow_alpha = if x > 0 && y > 0 {
                    alpha_data[(x - 1) + (y - 1) * dst_stride]
                } else {
                    0
                };

                let pixel_offset = (x + y * dst_stride) * 4;
                if shadow_alpha > 0 {
                    rgba_data[pixel_offset] = 0;
                    rgba_data[pixel_offset + 1] = 0;
                    rgba_data[pixel_offset + 2] = 0;
                    rgba_data[pixel_offset + 3] = shadow_alpha.saturating_sub(96);
                }

                if alpha > 0 {
                    rgba_data[pixel_offset] = 16;
                    rgba_data[pixel_offset + 1] = 16;
                    rgba_data[pixel_offset + 2] = 16;
                    rgba_data[pixel_offset + 3] = alpha;
                }
            }
        }

        let mut image = Image::new(
            Extent3d {
                width: target_texture_width,
                height: target_texture_height,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            rgba_data,
            TextureFormat::Rgba8Unorm,
        );
        image.sampler_descriptor = ImageSampler::Descriptor(ImageSampler::linear_descriptor());
        row_images.push((images.add(image), row_size / CHAT_BUBBLE_TEXT_RENDER_SCALE));
    }

    let total_text_height: f32 = row_images
        .iter()
        .map(|(_, display_size)| display_size.y)
        .sum::<f32>()
        + CHAT_BUBBLE_LINE_SPACING * row_images.len().saturating_sub(1) as f32;
    let mut current_y = background_offset.y + (background_height - total_text_height) * 0.5;

    for (image, display_size) in row_images {
        rows.push(WorldUiRect {
            image,
            screen_offset: Vec2::new(background_offset.x + CHAT_BUBBLE_TEXT_PADDING_X, current_y),
            screen_size: display_size,
            uv_min: Vec2::ZERO,
            uv_max: Vec2::ONE,
            color: Color::WHITE,
            order: CHAT_BUBBLE_ORDER_TEXT,
        });

        current_y += display_size.y + CHAT_BUBBLE_LINE_SPACING;
    }

    Some(ChatBubbleData { row_count, rows })
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
    egui_managed_textures: Res<EguiManagedTextures>,
    asset_server: Res<AssetServer>,
    mut images: ResMut<Assets<Image>>,
    model_loader: Res<ModelLoader>,
    time: Res<Time>,
    query_window: Query<Entity, With<PrimaryWindow>>,
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
    )>,
) {
    let Ok(window_entity) = query_window.get_single() else {
        return;
    };

    let pixels_per_point = egui_context.ctx_mut().pixels_per_point();
    let zone_changed = load_zone_events.iter().last().is_some();
    let dpi_changed = chat_bubble_cache.pixels_per_point != 0.0
        && (chat_bubble_cache.pixels_per_point - pixels_per_point).abs() > f32::EPSILON;

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
        chat_bubble_cache.pending.clear();
        chat_bubble_cache.pixels_per_point = pixels_per_point;

        for _ in world_chat_bubble_events.iter() {}
        return;
    } else if chat_bubble_cache.pixels_per_point == 0.0 {
        chat_bubble_cache.pixels_per_point = pixels_per_point;
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

    let pending_entities: Vec<_> = chat_bubble_cache.pending.keys().copied().collect();
    for owner_entity in pending_entities {
        let Some(pending_data) = chat_bubble_cache.pending.remove(&owner_entity) else {
            continue;
        };
        let cache_key = pending_data.cache_key.clone();
        let pending_data_clone = pending_data.clone();

        let Some(chat_bubble_data) = create_chat_bubble_data(
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

        let Ok((existing_bubble, model_height, equipment, vehicle)) = query_owner.get(owner_entity)
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

    for event in world_chat_bubble_events.iter() {
        let text = event.text.trim();
        if text.is_empty() {
            continue;
        }

        let Ok((existing_bubble, model_height, equipment, vehicle)) = query_owner.get(event.entity)
        else {
            continue;
        };

        let galley = create_chat_bubble_galley(egui_context.ctx_mut(), text, pixels_per_point);
        let row_count = retained_chat_bubble_row_count(&galley);
        if row_count == 0 {
            continue;
        }

        let cache_key = ChatBubbleCacheKey {
            text: text.to_string(),
            pixels_per_point_bits: pixels_per_point.to_bits(),
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

        chat_bubble_cache
            .pending
            .insert(event.entity, ChatBubblePendingData { cache_key, galley });
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

    let background_offset = bubble_background_offset(chat_bubble_data.row_count);
    let background_height = bubble_background_height(chat_bubble_data.row_count);
    let background_image = if chat_bubble_data.row_count > 1 {
        bubble_assets.big.clone_weak()
    } else {
        bubble_assets.small.clone_weak()
    };

    commands.entity(bubble_entity).with_children(|parent| {
        parent.spawn((
            WorldUiRect {
                image: background_image,
                screen_offset: background_offset,
                screen_size: Vec2::new(CHAT_BUBBLE_WIDTH, background_height),
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

        for rect in chat_bubble_data.rows.iter() {
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

#[cfg(test)]
mod tests {
    use super::{
        advance_chat_bubble_lifetime, create_chat_bubble_galley, reset_chat_bubble_lifetime,
        retained_chat_bubble_row_count, ChatBubbleLifetime, CHAT_BUBBLE_TEXT_MAX_ROWS,
    };

    #[test]
    fn chat_bubble_layout_is_capped_to_two_rows() {
        let context = egui::Context::default();
        context.set_pixels_per_point(1.0);
        let _ = context.run(Default::default(), |_| {});

        let galley = create_chat_bubble_galley(
            &context,
            "This is a long line of text that should wrap into more than two visual rows inside the chat bubble layout system.",
            1.0,
        );

        assert!(galley.rows.len() > CHAT_BUBBLE_TEXT_MAX_ROWS);
        assert_eq!(
            retained_chat_bubble_row_count(&galley),
            CHAT_BUBBLE_TEXT_MAX_ROWS
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
