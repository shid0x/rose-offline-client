use arrayvec::ArrayVec;
use bevy::ecs::query::With;
use bevy::{
    ecs::event::Events,
    math::{Quat, Vec3},
    prelude::{
        AssetServer, Commands, ComputedVisibility, DespawnRecursiveExt, Entity, EventWriter,
        GlobalTransform, Mut, NextState, Query, Res, ResMut, State, Transform, Visibility, World,
    },
};

use rose_data::{
    AbilityType, EquipmentItem, Item, ItemReference, ItemSlotBehaviour, ItemType, SkillCooldown,
    SoundId, StatusEffectType,
};
use rose_file_readers::VfsPathBuf;
use rose_game_common::{
    components::{
        AbilityValues, BasicStatType, BasicStats, CharacterInfo, ClanPoints, DroppedItem,
        Equipment, ExperiencePoints, HealthPoints, Hotbar, Inventory, ItemDrop, ItemSlot, Level,
        ManaPoints, Money, MoveMode, MoveSpeed, Npc, QuestState, RecoveryRateBonus, SkillList,
        Stamina, StatPoints, StatusEffects, StatusEffectsRegen,
    },
    messages::{
        client::ClientMessage,
        server::{
            ActiveStatusEffects, ClanCreateError, ClanUpgradeResult, LearnSkillError,
            LevelUpSkillError, PartyMemberInfo, PartyMemberInfoOffline,
            PersonalStoreTransactionStatus, PickupItemDropError, ServerMessage, SpawnCommandState,
        },
        PartyItemSharing, PartyXpSharing,
    },
};
use rose_network_common::ConnectionError;

use crate::{
    audio::SpatialSound,
    bundles::{ability_values_add_value_exclusive, ability_values_set_value_exclusive},
    components::{
        Bank, Clan, ClanMember, ClanMembership, ClientEntity, ClientEntityName, ClientEntityType,
        CollisionHeightOnly, CollisionPlayer, CollisionPlayerGrounding, Command,
        CommandCastSkillTarget, Cooldowns, Dead, FacingDirection, NextCommand, PartyInfo,
        PartyOwner, PassiveRecoveryTime, PendingDamage, PendingDamageList, PendingSkillEffect,
        PendingSkillEffectList, PendingSkillTarget, PendingSkillTargetList, PersonalStore,
        PlayerCharacter, Position, SoundCategory, SummonPoints, VisibleStatusEffects,
    },
    events::{
        BankEvent, ChatboxEvent, ClientEntityEvent, CraftEvent, GameConnectionEvent, HitEvent,
        LoadZoneEvent, MessageBoxEvent, PartyEvent, PersonalStoreEvent, QuestTriggerEvent,
        SkillHitSoundEvent, SpawnEffectData, SpawnEffectEvent, UseItemEvent, WorldChatBubbleEvent,
    },
    resources::{
        AppState, ClientEntityList, GameConnection, GameData, PendingClanInvites, SocialState,
        SoundCache, SoundSettings, WorldConnection, WorldRates, WorldTime,
    },
    ui::UiSoundEvent,
};

const BONFIRE_BASE_SKILL_ID: u16 = 1161;
const GET_ITEM_SOUND_ID: u16 = 531;

fn party_level_up_event(player_entity: Entity, is_level_up: bool) -> Option<ClientEntityEvent> {
    is_level_up.then_some(ClientEntityEvent::PartyLevelUp(player_entity))
}

fn apply_status_effect_updates(
    status_effects: &mut StatusEffects,
    update_status_effects: &ActiveStatusEffects,
    updated_values: &[i32],
) -> (Option<i32>, Option<i32>) {
    let mut updated_hp = None;
    let mut updated_mp = None;

    for (status_effect_type, active) in update_status_effects.iter() {
        match active {
            Some(active) => {
                status_effects.active[status_effect_type] = Some(active.clone());
                status_effects.expire_times[status_effect_type] = None;
            }
            None => {
                if status_effects.active[status_effect_type].is_some() {
                    match status_effect_type {
                        StatusEffectType::IncreaseHp => {
                            updated_hp = updated_values.first().cloned();
                        }
                        StatusEffectType::IncreaseMp => {
                            updated_mp = updated_values.last().cloned();
                        }
                        _ => {}
                    }
                }

                status_effects.active[status_effect_type] = None;
                status_effects.expire_times[status_effect_type] = None;
            }
        }
    }

    (updated_hp, updated_mp)
}

fn clear_missing_status_effect_regen(
    status_effects_regen: &mut StatusEffectsRegen,
    update_status_effects: &ActiveStatusEffects,
) {
    for (status_effect_type, active) in update_status_effects.iter() {
        if active.is_none() {
            status_effects_regen.regens[status_effect_type] = None;
        }
    }
}

fn queue_bonfire_cast_sound(
    commands: &mut Commands,
    game_data: &GameData,
    asset_server: &AssetServer,
    sound_cache: &SoundCache,
    sound_settings: &SoundSettings,
    query_global_transform: &Query<&GlobalTransform>,
    player_entity: Option<Entity>,
    entity: Entity,
    skill_id: rose_data::SkillId,
) {
    let Some(skill_data) = game_data.skills.get_skill(skill_id) else {
        return;
    };
    if skill_data.base_skill_id.unwrap_or(skill_data.id).get() != BONFIRE_BASE_SKILL_ID {
        return;
    }

    let Some(sound_data) = skill_data
        .bullet_fire_sound_id
        .and_then(|sound_id| game_data.sounds.get_sound(sound_id))
    else {
        return;
    };

    let Ok(global_transform) = query_global_transform.get(entity) else {
        return;
    };

    let sound_category = if player_entity == Some(entity) {
        SoundCategory::PlayerCombat
    } else {
        SoundCategory::OtherCombat
    };
    let translation = global_transform.translation();

    commands.spawn((
        sound_category,
        sound_settings.gain(sound_category),
        SpatialSound::new(sound_cache.load(sound_data, asset_server)),
        Transform::from_translation(translation),
        GlobalTransform::from_translation(translation),
    ));
}

fn to_next_command(
    command_state: &SpawnCommandState,
    client_entity_list: &ClientEntityList,
) -> NextCommand {
    match *command_state {
        SpawnCommandState::Move {
            target_position,
            target_entity_id,
        } => NextCommand::with_move(
            target_position,
            target_entity_id.and_then(|id| client_entity_list.get(id)),
            None,
        ),
        SpawnCommandState::RunAway { target_position } => {
            NextCommand::with_move(target_position, None, Some(MoveMode::Run))
        }
        SpawnCommandState::Attack {
            target_entity_id, ..
        } => {
            if let Some(target_entity) = client_entity_list.get(target_entity_id) {
                NextCommand::with_attack(target_entity)
            } else {
                NextCommand::default()
            }
        }
        SpawnCommandState::Sit => NextCommand::with_sitting(),
        SpawnCommandState::PersonalStore => NextCommand::with_personal_store(),
        SpawnCommandState::Die => NextCommand::with_die(),
        _ => NextCommand::default(),
    }
}

fn update_inventory_and_money(
    world: &mut World,
    player_entity: Entity,
    update_items: Vec<(ItemSlot, Option<Item>)>,
    update_money: Option<Money>,
) {
    let mut player = world.entity_mut(player_entity);

    if let Some(mut inventory) = player.get_mut::<Inventory>() {
        for (item_slot, item) in update_items.iter() {
            if let ItemSlot::Inventory(_, _) = item_slot {
                if let Some(item_slot) = inventory.get_item_slot_mut(*item_slot) {
                    *item_slot = item.clone();
                }
            }
        }

        if let Some(money) = update_money {
            inventory.money = money;
        }
    }

    if let Some(mut equipment) = player.get_mut::<Equipment>() {
        for (item_slot, item) in update_items.iter() {
            match *item_slot {
                ItemSlot::Ammo(ammo_index) => {
                    *equipment.get_ammo_slot_mut(ammo_index) =
                        item.as_ref().and_then(|x| x.as_stackable().cloned())
                }
                ItemSlot::Equipment(equipment_index) => {
                    *equipment.get_equipment_slot_mut(equipment_index) =
                        item.as_ref().and_then(|x| x.as_equipment().cloned())
                }
                ItemSlot::Vehicle(vehicle_part_index) => {
                    *equipment.get_vehicle_slot_mut(vehicle_part_index) =
                        item.as_ref().and_then(|x| x.as_equipment().cloned())
                }
                _ => {}
            }
        }
    }
}

fn clear_visible_character_clan_membership_by_name(world: &mut World, name: &str) {
    let mut query = world.query::<(Entity, &ClientEntity, &ClientEntityName)>();
    let entities_to_clear = query
        .iter(world)
        .filter_map(|(entity, client_entity, client_entity_name)| {
            (client_entity.entity_type == ClientEntityType::Character
                && client_entity_name.name == name)
                .then_some(entity)
        })
        .collect::<Vec<_>>();

    for entity in entities_to_clear {
        if let Some(mut entity_mut) = world.get_entity_mut(entity) {
            entity_mut.remove::<ClanMembership>();
        }
    }
}

fn reward_money_diff(current_money: Money, new_total_money: Money) -> i64 {
    new_total_money.0 - current_money.0
}

fn sync_reward_money(inventory: &mut Inventory, new_total_money: Money) -> i64 {
    let diff = reward_money_diff(inventory.money, new_total_money);
    inventory.money = new_total_money;
    diff
}

pub fn game_connection_system(
    mut commands: Commands,
    game_connection: Option<Res<GameConnection>>,
    game_data: Res<GameData>,
    app_state_current: Res<State<AppState>>,
    mut app_state_next: ResMut<NextState<AppState>>,
    mut client_entity_list: ResMut<ClientEntityList>,
    (
        mut chatbox_events,
        mut game_connection_events,
        mut load_zone_events,
        mut use_item_events,
        mut client_entity_events,
        mut ui_sound_events,
    ): (
        EventWriter<ChatboxEvent>,
        EventWriter<GameConnectionEvent>,
        EventWriter<LoadZoneEvent>,
        EventWriter<UseItemEvent>,
        EventWriter<ClientEntityEvent>,
        EventWriter<UiSoundEvent>,
    ),
    query_player_inventory: Query<&Inventory, With<PlayerCharacter>>,
    (
        mut party_events,
        mut personal_store_events,
        mut quest_trigger_events,
        mut message_box_events,
    ): (
        EventWriter<PartyEvent>,
        EventWriter<PersonalStoreEvent>,
        EventWriter<QuestTriggerEvent>,
        EventWriter<MessageBoxEvent>,
    ),
    (asset_server, sound_cache, sound_settings, query_global_transform): (
        Res<AssetServer>,
        Res<SoundCache>,
        Res<SoundSettings>,
        Query<&GlobalTransform>,
    ),
    (world_connection, mut pending_clan_invites, mut spawn_effect_events): (
        Option<Res<WorldConnection>>,
        ResMut<PendingClanInvites>,
        EventWriter<SpawnEffectEvent>,
    ),
    mut social_state: ResMut<SocialState>,
) {
    let Some(game_connection) = game_connection else {
        return;
    };

    let result: Result<(), anyhow::Error> = loop {
        match game_connection.server_message_rx.try_recv() {
            Ok(ServerMessage::ConnectionRequestSuccess { .. }) =>{
            client_entity_list.clear();
            social_state.clear();
            }
            Ok(ServerMessage::ConnectionRequestError { .. }) =>{
                break Err(ConnectionError::ConnectionLost.into());
            },
            Ok(ServerMessage::CharacterData { data: character_data }) => {
                let status_effects = StatusEffects::default();
                let ability_values = game_data.ability_value_calculator.calculate(
                    &character_data.character_info,
                    &character_data.level,
                    &character_data.equipment,
                    &character_data.basic_stats,
                    &character_data.skill_list,
                    &status_effects,
                );
                let move_mode = MoveMode::Run;
                let move_speed = MoveSpeed::new(ability_values.get_move_speed(&move_mode));

                // Spawn character
                client_entity_list.player_entity = Some(
                    commands
                        .spawn(((
                            PlayerCharacter {},
                            ClientEntityName::new(character_data.character_info.name.clone()),
                            character_data.character_info,
                            character_data.basic_stats,
                            character_data.level,
                            character_data.equipment,
                            character_data.experience_points,
                            character_data.skill_list,
                            character_data.hotbar,
                            character_data.health_points,
                            character_data.mana_points,
                            character_data.stat_points,
                            character_data.skill_points,
                            character_data.union_membership,
                            character_data.stamina,
                        ),
                        (
                            Command::with_stop(),
                            NextCommand::with_stop(),
                            FacingDirection::default(),
                            ability_values,
                            status_effects,
                            StatusEffectsRegen::new(),
                            move_mode,
                            move_speed,
                            Cooldowns::default(),
                            PassiveRecoveryTime::default(),
                            PendingSkillTargetList::default(),
                            PendingDamageList::default(),
                            PendingSkillEffectList::default(),
                            Position::new(character_data.position),
                            VisibleStatusEffects::default(),
                        ),
                        (
                            RecoveryRateBonus::default(),
                            SummonPoints::default(),
                            Transform::from_xyz(
                                character_data.position.x / 100.0,
                                character_data.position.z / 100.0 + 100.0,
                                -character_data.position.y / 100.0,
                            ),
                            GlobalTransform::default(),
                            Visibility::default(),
                            ComputedVisibility::default(),
                        )))
                        .id()
                );

                // Emit connected event, character select system will be responsible for
                // starting the load of the next zone once its animations have completed
                game_connection_events.send(GameConnectionEvent::Connected(character_data.zone_id));
                client_entity_list.zone_id = Some(character_data.zone_id);
            }
            Ok(ServerMessage::CharacterDataItems { data }) => {
                if let Some(player_entity) = client_entity_list.player_entity {
                    commands
                        .entity(player_entity)
                        .insert((data.inventory, data.equipment));
                }
            }
            Ok(ServerMessage::CharacterDataQuest { quest_state }) => {
                if let Some(player_entity) = client_entity_list.player_entity {
                    commands.entity(player_entity).insert(*quest_state);
                }
            }
            Ok(ServerMessage::FriendList { friends }) => {
                social_state.friends = friends;
            }
            Ok(ServerMessage::FriendAddRequest { requester_id, name }) => {
                if !social_state
                    .pending_requests
                    .iter()
                    .any(|request| request.requester_id == requester_id)
                {
                    social_state.pending_requests.push(crate::resources::PendingFriendRequest {
                        requester_id,
                        name,
                    });
                }
            }
            Ok(ServerMessage::FriendAdded { friend }) => {
                social_state.upsert_friend(friend);
            }
            Ok(ServerMessage::FriendAddRejected { name }) => {
                message_box_events.send(MessageBoxEvent::Show {
                    message: format!("{} rejected your friend request.", name),
                    modal: false,
                    ok: None,
                    cancel: None,
                });
            }
            Ok(ServerMessage::FriendAddTargetNotFound { name }) => {
                message_box_events.send(MessageBoxEvent::Show {
                    message: format!("{} is not online.", name),
                    modal: false,
                    ok: None,
                    cancel: None,
                });
            }
            Ok(ServerMessage::FriendRemoved { friend_id }) => {
                social_state.remove_friend(friend_id);
            }
            Ok(ServerMessage::FriendStatusChanged { friend_id, status }) => {
                if matches!(status, rose_game_common::messages::FriendStatus::Deleted) {
                    social_state.remove_friend(friend_id);
                } else {
                    social_state.update_friend_status(friend_id, status);
                }
            }
            Ok(ServerMessage::FriendChat {
                friend_id,
                from_name,
                text,
            }) => {
                let sender_name = if from_name.is_empty() {
                    social_state
                        .get_friend(friend_id)
                        .map(|friend| friend.name.clone())
                        .unwrap_or_else(|| "Friend".to_string())
                } else {
                    from_name
                };
                social_state.append_chat_message(friend_id, sender_name, text, false);
                social_state.request_open_chat(friend_id);
            }
            Ok(ServerMessage::JoinZone { entity_id, experience_points, team, global_flags: _, health_points, mana_points, world_ticks, craft_rate, world_price_rate, item_price_rate, town_price_rate }) => {
                if let Some(player_entity) = client_entity_list.player_entity {
                    let mut entity_commands = commands.entity(player_entity);
                    entity_commands.insert((
                        ClientEntity::new(entity_id, ClientEntityType::Character),
                        CollisionPlayer,
                        Command::with_stop(),
                        NextCommand::with_stop(),
                        FacingDirection::default(),
                        experience_points,
                        team,
                        health_points,
                        mana_points,
                    ));

                    if health_points.hp > 0 {
                        entity_commands.remove::<Dead>();
                    } else {
                        entity_commands.insert((Dead, Command::with_die(), NextCommand::default()));
                    }

                    commands.insert_resource(WorldRates {
                        craft_rate,
                        item_price_rate,
                        world_price_rate,
                        town_price_rate,
                    });
                    commands.insert_resource(WorldTime::new(world_ticks));

                    client_entity_list.clear();
                    client_entity_list.add(entity_id, player_entity);
                    client_entity_list.player_entity_id = Some(entity_id);

                    // Transition to in game state if we are not already
                    if !matches!(app_state_current.get(), AppState::Game) {
                        app_state_next.set(AppState::Game);
                    }
                }
            }
            Ok(ServerMessage::SpawnEntityCharacter { data: message  }) => {
                let status_effects = StatusEffects {
                    active: message.status_effects,
                    ..Default::default()
                };
                let next_command = to_next_command(&message.spawn_command_state, &client_entity_list);
                let mut ability_values = game_data.ability_value_calculator.calculate(
                    &message.character_info,
                    &message.level,
                    &message.equipment,
                    &BasicStats::default(),
                    &SkillList::default(),
                    &status_effects,
                );
                ability_values.run_speed = message.move_speed.speed;
                ability_values.attack_speed += message.passive_attack_speed;
                ability_values.passive_attack_speed = message.passive_attack_speed;

                let entity = commands
                    .spawn(((
                        ClientEntityName::new(message.character_info.name.clone()),
                        Command::with_stop(),
                        next_command,
                        message.character_info,
                        message.team,
                        message.health,
                        message.move_mode,
                        Position::new(message.position),
                        message.equipment,
                        message.level,
                        message.move_speed,
                        ability_values,
                        status_effects,
                        StatusEffectsRegen::new(),
                    ),
                    (
                        ClientEntity::new(message.entity_id, ClientEntityType::Character),
                        CollisionHeightOnly,
                        FacingDirection::default(),
                        PendingDamageList::default(),
                        PendingSkillEffectList::default(),
                        PendingSkillTargetList::default(),
                        Transform::from_xyz(
                            message.position.x / 100.0,
                            message.position.z / 100.0 + 10000.0,
                            -message.position.y / 100.0,
                        ),
                        GlobalTransform::default(),
                        Visibility::default(),
                        ComputedVisibility::default(),
                        VisibleStatusEffects::default(),
                    ),))
                    .id();

                if let Some((skin, title)) = message.personal_store_info {
                    commands
                        .entity(entity)
                        .insert(PersonalStore::new(title, skin as usize));
                }

                if let Some(clan_membership) = message.clan_membership {
                    commands
                        .entity(entity)
                        .insert(ClanMembership {
                            clan_unique_id: clan_membership.clan_unique_id,
                            mark: clan_membership.mark,
                            level: clan_membership.level,
                            name: clan_membership.name,
                            position: clan_membership.position,
                            contribution: ClanPoints(0),
                        });
                }

                client_entity_list.add(message.entity_id, entity);
            }
            Ok(ServerMessage::SpawnEntityNpc {
                entity_id,
                npc,
                direction,
                position,
                team,
                health,
                spawn_command_state,
                move_mode,
                status_effects,
            }) => {
                let status_effects = StatusEffects {
                    active: status_effects,
                    ..Default::default()
                };
                let ability_values = game_data
                    .ability_value_calculator
                    .calculate_npc(npc.id, &status_effects, None, None)
                    .unwrap();
                let move_speed = MoveSpeed::new(ability_values.get_move_speed(&move_mode));
                let level = Level::new(ability_values.get_level() as u32);
                let next_command = to_next_command(&spawn_command_state, &client_entity_list);

                let entity = commands
                    .spawn((
                        (
                        Command::with_stop(),
                        next_command,
                        npc,
                        team,
                        health,
                        move_mode,
                        Position::new(position),
                        ability_values,
                        level,
                        move_speed,
                        status_effects,
                        StatusEffectsRegen::new(),
                    ), (
                        ClientEntity::new(entity_id, ClientEntityType::Npc),
                        CollisionHeightOnly,
                        FacingDirection::default(),
                        PendingDamageList::default(),
                        PendingSkillEffectList::default(),
                        PendingSkillTargetList::default(),
                        VisibleStatusEffects::default(),
                        Transform::from_xyz(
                            position.x / 100.0,
                            position.z / 100.0 + 10000.0,
                            -position.y / 100.0,
                        )
                        .with_rotation(Quat::from_axis_angle(
                            Vec3::Y,
                            direction.to_radians(),
                        )),
                        GlobalTransform::default(),
                        Visibility::default(),
                        ComputedVisibility::default(),
                    ),
                    ))
                    .id();

                client_entity_list.add(entity_id, entity);
            }
            Ok(ServerMessage::SpawnEntityMonster { entity_id, npc, position, team, health, spawn_command_state, move_mode, status_effects }) => {
                let status_effects = StatusEffects {
                    active: status_effects,
                    ..Default::default()
                };
                let ability_values = game_data
                    .ability_value_calculator
                    .calculate_npc(npc.id, &status_effects, None, None)
                    .unwrap();
                let move_speed = MoveSpeed::new(ability_values.get_move_speed(&move_mode));
                let level = Level::new(ability_values.get_level() as u32);
                let next_command = to_next_command(&spawn_command_state, &client_entity_list);

                let mut equipment = Equipment::new();
                if let Some(npc_data) = game_data.npcs.get_npc(npc.id) {
                    if npc_data.right_hand_part_index > 0 {
                        equipment
                            .equip_item(
                                EquipmentItem::new(
                                    ItemReference::new(
                                        ItemType::Weapon,
                                        npc_data.right_hand_part_index as usize,
                                    ),
                                    0,
                                )
                                .unwrap(),
                            )
                            .ok();
                    }

                    if npc_data.left_hand_part_index > 0 {
                        equipment
                            .equip_item(
                                EquipmentItem::new(
                                    ItemReference::new(
                                        ItemType::SubWeapon,
                                        npc_data.left_hand_part_index as usize,
                                    ),
                                    0,
                                )
                                .unwrap(),
                            )
                            .ok();
                    }
                }

                let entity = commands
                    .spawn(((
                        Command::with_stop(),
                        next_command,
                        npc,
                        team,
                        health,
                        move_mode,
                        Position::new(position),
                        ability_values,
                        equipment,
                        level,
                        move_speed,
                        status_effects,
                        StatusEffectsRegen::new(),
                    ),
                    (
                        ClientEntity::new(entity_id, ClientEntityType::Monster),
                        CollisionHeightOnly,
                        FacingDirection::default(),
                        PendingDamageList::default(),
                        PendingSkillEffectList::default(),
                        PendingSkillTargetList::default(),
                        VisibleStatusEffects::default(),
                        Transform::from_xyz(
                            position.x / 100.0,
                            position.z / 100.0 + 10000.0,
                            -position.y / 100.0,
                        ),
                        GlobalTransform::default(),
                        Visibility::default(),
                        ComputedVisibility::default(),
                    ),))
                    .id();

                client_entity_list.add(entity_id, entity);
            }
            Ok(ServerMessage::SpawnEntityItemDrop { entity_id, dropped_item, position, remaining_time: _, owner_entity_id: _ }) => {
                let name = match &dropped_item {
                    DroppedItem::Item(item) => game_data
                        .items
                        .get_base_item(item.get_item_reference())
                        .map(|item_data| item_data.name.to_string())
                        .unwrap_or_else(|| {
                            format!("[{:?} {}]", item.get_item_type(), item.get_item_number())
                        }),
                    DroppedItem::Money(money) => {
                        format!("{} Zuly", money.0)
                    }
                };

                // TODO: Use message.remaining_time, message.owner_entity_id ?
                let entity = commands
                    .spawn((
                        ClientEntityName::new(name),
                        ItemDrop::with_dropped_item(dropped_item),
                        Position::new(position),
                        ClientEntity::new(entity_id, ClientEntityType::ItemDrop),
                        CollisionHeightOnly,
                        Transform::from_xyz(
                            position.x / 100.0,
                            position.z / 100.0 + 10000.0,
                            -position.y / 100.0,
                        ),
                        GlobalTransform::default(),
                        Visibility::default(),
                        ComputedVisibility::default(),
                    ))
                    .id();

                client_entity_list.add(entity_id, entity);
            }
            Ok(ServerMessage::MoveEntity { entity_id, target_entity_id, distance: _, x, y, z, move_mode }) => {
                if let Some(entity) = client_entity_list.get(entity_id) {
                    let target_entity = target_entity_id
                        .and_then(|id| client_entity_list.get(id));

                    commands.entity(entity).insert(NextCommand::with_move(
                        Vec3::new(x, y, z as f32),
                        target_entity,
                        move_mode,
                    ));
                }
            }
            Ok(ServerMessage::AdjustPosition { entity_id, position }) => {
                if let Some(entity) = client_entity_list.get(entity_id) {
                    let is_player_entity = client_entity_list.player_entity_id == Some(entity_id);

                    if is_player_entity {
                        commands.entity(entity).remove::<Dead>().insert((
                            Position::new(position),
                            Transform::from_xyz(
                                position.x / 100.0,
                                position.z / 100.0,
                                -position.y / 100.0,
                            ),
                            CollisionPlayerGrounding,
                            Command::with_stop(),
                            NextCommand::with_stop(),
                            PendingDamageList::default(),
                            PendingSkillEffectList::default(),
                            PendingSkillTargetList::default(),
                        ));
                    } else {
                        commands
                            .entity(entity)
                            .insert(NextCommand::with_move(position, None, None));
                    }
                }
            }
            Ok(ServerMessage::StopMoveEntity { entity_id, x: _, y: _, z: _ }) => {
                // TODO: Lerp to XYZ ?
                if let Some(entity) = client_entity_list.get(entity_id) {
                    commands.entity(entity).insert(NextCommand::with_stop());
                }
            }
            Ok(ServerMessage::AttackEntity {
                entity_id,
                target_entity_id,
                distance: _,
                x: _,
                y: _,
                z: _,
            }) => {
                if let Some(entity) = client_entity_list.get(entity_id) {
                    if let Some(target_entity) = client_entity_list.get(target_entity_id) {
                        commands
                            .entity(entity)
                            .insert(NextCommand::with_attack(target_entity));
                    }
                }
            }
            Ok(ServerMessage::RemoveEntities { entity_ids }) => {
                for entity_id in entity_ids {
                    if let Some(entity) = client_entity_list.get(entity_id) {
                        client_entity_list.remove(entity_id);
                        commands.entity(entity).despawn_recursive();
                    }
                }
            }
            Ok(ServerMessage::DamageEntity { attacker_entity_id, defender_entity_id, damage, is_killed, is_immediate, from_skill }) => {
                if let Some(defender_entity) = client_entity_list.get(defender_entity_id) {
                    let attacker_entity = client_entity_list.get(attacker_entity_id);
                    let killed_by_player = is_killed
                        && client_entity_list.player_entity
                            == client_entity_list.get(attacker_entity_id);
                    let aoe_skill_id = if let Some((skill_id, _)) = from_skill.as_ref() {
                        match game_data.skills.get_skill(*skill_id) {
                            Some(skill_data) if skill_data.scope > 0 => Some(*skill_id),
                            Some(_) => None,
                            None => {
                                log::warn!(
                                    "Received skill damage for unknown skill id {}",
                                    skill_id.get()
                                );
                                None
                            }
                        }
                    } else {
                        None
                    };

                    commands.add(move |world: &mut World| {
                        if let Some(skill_id) = aoe_skill_id {
                            if let Some(attacker_entity) = attacker_entity {
                                let explicit_target = if let Some(attacker) =
                                    world.get_entity(attacker_entity)
                                {
                                    if let Some(command) = attacker.get::<Command>() {
                                        command.get_target()
                                    } else {
                                        log::debug!(
                                            "Missing command component for AOE attacker entity {:?}",
                                            attacker_entity
                                        );
                                        None
                                    }
                                } else {
                                    log::debug!(
                                        "AOE attacker entity {:?} no longer exists locally",
                                        attacker_entity
                                    );
                                    None
                                };

                                let is_primary_target = explicit_target == Some(defender_entity)
                                    || defender_entity == attacker_entity;

                                if !is_primary_target {
                                    log::debug!(
                                        "Emitting synthetic AOE hit feedback for skill {} attacker {:?} defender {:?}",
                                        skill_id.get(),
                                        attacker_entity,
                                        defender_entity
                                    );
                                    world.resource_mut::<Events<HitEvent>>().send(
                                        HitEvent::with_skill_damage(
                                            attacker_entity,
                                            defender_entity,
                                            skill_id,
                                        ),
                                    );
                                    world
                                        .resource_mut::<Events<SkillHitSoundEvent>>()
                                        .send(SkillHitSoundEvent::new(
                                            attacker_entity,
                                            defender_entity,
                                            skill_id,
                                        ));
                                }
                            } else {
                                log::warn!(
                                    "Unable to emit synthetic AOE hit feedback for skill {}: attacker entity id {:?} was not found on client",
                                    skill_id.get(),
                                    attacker_entity_id
                                );
                            }
                        }

                        let mut defender = world.entity_mut(defender_entity);
                        if let Some(mut pending_damage_list) =
                            defender.get_mut::<PendingDamageList>()
                        {
                            pending_damage_list.push(PendingDamage::new(
                                attacker_entity,
                                damage,
                                is_killed,
                                is_immediate,
                                from_skill,
                            ));
                        }

                        if killed_by_player {
                            if let Some(name) = defender.get::<ClientEntityName>() {
                                let chat_message =
                                    format!("You have succeeded in hunting {}", name.as_str());
                                world
                                    .resource_mut::<Events<ChatboxEvent>>()
                                    .send(ChatboxEvent::System(chat_message));
                            }
                        }
                    });
                }
            }
            Ok(ServerMessage::Teleport { entity_id: _, zone_id, x, y, run_mode: _, ride_mode: _ }) => {
                if let Some(player_entity) = client_entity_list.player_entity {
                    // Update player position
                    commands
                        .entity(player_entity)
                        .insert((
                            Position::new(Vec3::new(x, y, 0.0)),
                            Transform::from_xyz(x / 100.0, 100.0, -y / 100.0),
                        ))
                        .remove::<ClientEntity>()
                        .remove::<CollisionPlayer>();

                    // Despawn all non-player entities
                    for (client_entity_id, client_entity) in
                        client_entity_list.client_entities.iter().enumerate()
                    {
                        if let Some(client_entity) = client_entity {
                            if client_entity_list
                                .player_entity_id
                                .map_or(true, |id| id.0 != client_entity_id)
                            {
                                commands.entity(*client_entity).despawn_recursive();
                            }
                        }
                    }
                    client_entity_list.clear();

                    // Load next zone
                    load_zone_events.send(LoadZoneEvent::new(zone_id));
                    client_entity_list.zone_id = Some(zone_id);
                }
            }
            Ok(ServerMessage::LocalChat {
                entity_id,
                text,
            }) => {
                if let Some(chat_entity) = client_entity_list.get(entity_id) {
                    commands.add(move |world: &mut World| {
                        if let Some(name) = world.entity(chat_entity).get::<ClientEntityName>() {
                            let name = name.to_string();
                            world.resource_mut::<Events<ChatboxEvent>>().send(ChatboxEvent::Say(
                                name,
                                text.clone(),
                            ));
                            world
                                .resource_mut::<Events<WorldChatBubbleEvent>>()
                                .send(WorldChatBubbleEvent {
                                    entity: chat_entity,
                                    text,
                                });
                        }
                    });
                }
            }
            Ok(ServerMessage::ShoutChat { name, text }) => {
                chatbox_events.send(ChatboxEvent::Shout(name, text));
            }
            Ok(ServerMessage::Whisper { from, text }) => {
                chatbox_events.send(ChatboxEvent::Whisper(from, text));
            }
            Ok(ServerMessage::AnnounceChat { name, text }) => {
                chatbox_events.send(ChatboxEvent::Announce(name, text));
            }
            Ok(ServerMessage::UpdateAbilityValueAdd { ability_type, value }) => {
                if let Some(player_entity) = client_entity_list.player_entity {
                    chatbox_events.send(ChatboxEvent::System(format!(
                        "Ability {:?} has {} by {}.",
                        ability_type,
                        if value < 0 {
                            "decreased"
                        } else {
                            "increased"
                        },
                        value.abs(),
                    )));

                    commands.add(move |world: &mut World| {
                        let mut player = world.entity_mut(player_entity);
                        ability_values_add_value_exclusive(
                            ability_type,
                            value,
                            &mut player,
                        );
                    });
                }
            }
            Ok(ServerMessage::UpdateAbilityValueSet { ability_type, value }) => {
                if let Some(player_entity) = client_entity_list.player_entity {
                    if !matches!(ability_type, AbilityType::Health | AbilityType::Mana) {
                        chatbox_events.send(ChatboxEvent::System(format!(
                            "Ability {:?} has been changed to {}.",
                            ability_type, value,
                        )));
                    }

                    commands.add(move |world: &mut World| {
                        let mut player = world.entity_mut(player_entity);
                        ability_values_set_value_exclusive(
                            ability_type,
                            value,
                            &mut player,
                        );
                    });
                }
            }
            Ok(ServerMessage::UpdateAmmo { entity_id, ammo_index, item }) => {
                if let Some(entity) = client_entity_list.get(entity_id) {
                    commands.add(move |world: &mut World| {
                        if let Some(mut equipment) = world.entity_mut(entity).get_mut::<Equipment>()
                        {
                            *equipment.get_ammo_slot_mut(ammo_index) = item;
                        }
                    });
                }
            }
            Ok(ServerMessage::UpdateEquipment { entity_id, equipment_index, item  }) => {
                if let Some(entity) = client_entity_list.get(entity_id) {
                    commands.add(move |world: &mut World| {
                        if let Some(mut equipment) = world.entity_mut(entity).get_mut::<Equipment>()
                        {
                            if let Some(equipped_item) =
                                equipment.equipped_items[equipment_index].as_mut()
                            {
                                if let Some(item) = item {
                                    // Only update visual related data
                                    equipped_item.item = item.item;
                                    equipped_item.has_socket = item.has_socket;
                                    equipped_item.gem = item.gem;
                                    equipped_item.grade = item.grade;
                                } else {
                                    equipment.equipped_items[equipment_index] = None;
                                }
                            } else {
                                equipment.equipped_items[equipment_index] = item;
                            }
                        }
                    });
                }
            }
            Ok(ServerMessage::UpdateVehiclePart { entity_id, vehicle_part_index, item }) => {
                if let Some(entity) = client_entity_list.get(entity_id) {
                    commands.add(move |world: &mut World| {
                        if let Some(mut equipment) = world.entity_mut(entity).get_mut::<Equipment>()
                        {
                            if let Some(equipped_item) =
                                equipment.equipped_vehicle[vehicle_part_index].as_mut()
                            {
                                if let Some(item) = item {
                                    // Only update visual related data
                                    equipped_item.item = item.item;
                                    equipped_item.has_socket = item.has_socket;
                                    equipped_item.gem = item.gem;
                                    equipped_item.grade = item.grade;
                                } else {
                                    equipment.equipped_vehicle[vehicle_part_index] = None;
                                }
                            } else {
                                equipment.equipped_vehicle[vehicle_part_index] = item;
                            }
                        }
                    });
                }
            }
            Ok(ServerMessage::UpdateItemLife { item_slot, life }) => {
                if let Some(entity) = client_entity_list.player_entity {
                    commands.add(move |world: &mut World| {
                        match item_slot {
                            ItemSlot::Equipment(index) => {
                                if let Some(mut equipment) = world.entity_mut(entity).get_mut::<Equipment>() {
                                    if let Some(equipment_item) = equipment.get_equipment_item_mut(index) {
                                        equipment_item.life = life;
                                    }
                                }
                            },
                            ItemSlot::Vehicle(index) => {
                                if let Some(mut equipment) = world.entity_mut(entity).get_mut::<Equipment>() {
                                    if let Some(equipment_item) = equipment.get_vehicle_item_mut(index) {
                                        equipment_item.life = life;
                                    }
                                }
                            },
                            ItemSlot::Inventory(inventory_page, inventory_slot) => {
                                if let Some(mut inventory) =  world.entity_mut(entity).get_mut::<Inventory>() {
                                    if let Some(item) = inventory.get_item_mut(ItemSlot::Inventory(inventory_page, inventory_slot)) {
                                        match item {
                                            Item::Equipment(equipment_item) => equipment_item.life = life,
                                            Item::Stackable(_) => {},
                                        }
                                    }
                                }
                            },
                            ItemSlot::Ammo(_) => {},
                        }
                    });
                }
            }
            Ok(ServerMessage::UpdateInventory { items, money }) => {
                if let Some(player_entity) = client_entity_list.player_entity {
                    commands.add(move |world: &mut World| {
                        // Play equip sound for any vehicle part that was equipped
                        let equip_sound_id = {
                            let game_data = world.resource::<GameData>();
                            items.iter().find_map(|(slot, item)| {
                                if let ItemSlot::Vehicle(_) = slot {
                                    item.as_ref()
                                        .and_then(|i| {
                                            game_data
                                                .items
                                                .get_base_item(i.get_item_reference())
                                        })
                                        .and_then(|data| data.equip_sound_id)
                                } else {
                                    None
                                }
                            })
                        };

                        update_inventory_and_money(
                            world,
                            player_entity,
                            items,
                            money,
                        );

                        if let Some(sound_id) = equip_sound_id {
                            world.send_event(UiSoundEvent::new(sound_id));
                        }
                    });
                }
            }
            Ok(ServerMessage::UseInventoryItem { inventory_slot, .. }) => {
                if let Some(player_entity) = client_entity_list.player_entity {
                    commands.add(move |world: &mut World| {
                        if let Some(mut inventory) =
                            world.entity_mut(player_entity).get_mut::<Inventory>()
                        {
                            if let Some(item_slot) =
                                inventory.get_item_slot_mut(inventory_slot)
                            {
                                item_slot.try_take_quantity(1);
                            }
                        }
                    });
                }
            }
            Ok(ServerMessage::UpdateMoney { money }) => {
                if let Some(player_entity) = client_entity_list.player_entity {
                    commands.add(move |world: &mut World| {
                        if let Some(mut inventory) =
                            world.entity_mut(player_entity).get_mut::<Inventory>()
                        {
                            inventory.money = money;
                        }
                    });
                }
            }
            Ok(ServerMessage::UpdateBasicStat { basic_stat_type, value }) => {
                if let Some(player_entity) = client_entity_list.player_entity {
                    commands.add(move |world: &mut World| {
                        world.resource_scope(|world, game_data: Mut<GameData>| {
                            let mut stat_point_cost = None;
                            let mut player = world.entity_mut(player_entity);

                            if let Some(mut basic_stats) = player.get_mut::<BasicStats>() {
                                let current_value = basic_stats.get(basic_stat_type);

                                // Calculate stat point cost if this looked like a user requested stat increase
                                if value == current_value + 1 {
                                    if let Some(cost) = game_data
                                        .ability_value_calculator
                                        .calculate_basic_stat_increase_cost(
                                            &basic_stats,
                                            basic_stat_type,
                                        )
                                    {
                                        stat_point_cost = Some(cost);
                                    }
                                }

                                // Update stats
                                match basic_stat_type {
                                    BasicStatType::Strength => basic_stats.strength = value,
                                    BasicStatType::Dexterity => {
                                        basic_stats.dexterity = value
                                    }
                                    BasicStatType::Intelligence => {
                                        basic_stats.intelligence = value
                                    }
                                    BasicStatType::Concentration => {
                                        basic_stats.concentration = value
                                    }
                                    BasicStatType::Charm => basic_stats.charm = value,
                                    BasicStatType::Sense => basic_stats.sense = value,
                                }
                            }

                            // Update stat points
                            if let Some(subtract_stat_points) = stat_point_cost {
                                if let Some(mut stat_points) = player.get_mut::<StatPoints>() {
                                    stat_points.points -=
                                        stat_points.points.min(subtract_stat_points);
                                }
                            }
                        });
                    });
                }
            }
            Ok(ServerMessage::UpdateLevel { entity_id, level, experience_points, stat_points, skill_points }) => {
                if let Some(entity) = client_entity_list.get(entity_id) {
                    client_entity_events.send(ClientEntityEvent::LevelUp(
                        entity,
                        Some(level.level),
                    ));

                    commands.entity(entity).insert((
                        level,
                        experience_points,
                        stat_points,
                        skill_points,
                    ));

                    // Update HP / MP to max for new level
                    commands.add(move |world: &mut World| {
                        world.resource_scope(|world, game_data: Mut<GameData>| {
                            let mut character = world.entity_mut(entity);

                            if let (
                                Some(basic_stats),
                                Some(character_info),
                                Some(equipment),
                                Some(skill_list),
                                Some(status_effects),
                            ) = (
                                character.get::<BasicStats>(),
                                character.get::<CharacterInfo>(),
                                character.get::<Equipment>(),
                                character.get::<SkillList>(),
                                character.get::<StatusEffects>(),
                            ) {
                                let ability_values = game_data.ability_value_calculator.calculate(
                                    character_info,
                                    &level,
                                    equipment,
                                    basic_stats,
                                    skill_list,
                                    status_effects,
                                );

                                if let Some(mut health_points) = character.get_mut::<HealthPoints>()
                                {
                                    health_points.hp = ability_values.get_max_health();
                                }

                                if let Some(mut mana_points) = character.get_mut::<ManaPoints>() {
                                    mana_points.mp = ability_values.get_max_health();
                                }
                            }
                        });
                    });
                }
            }
            Ok(ServerMessage::LevelUpEntity { entity_id }) => {
                if client_entity_list.player_entity_id == Some(entity_id) {
                    // Ignore, the server erroneously sends this message in addition to ServerMessage::UpdateLevel
                } else if let Some(entity) = client_entity_list.get(entity_id) {
                    client_entity_events.send(ClientEntityEvent::LevelUp(entity, None));

                    commands.add(move |world: &mut World| {
                        world.resource_scope(|world, game_data: Mut<GameData>| {
                            let mut character = world.entity_mut(entity);

                            // Update level
                            if let Some(mut level) = character.get_mut::<Level>() {
                                level.level += 1;
                            }

                            // Update HP / MP to max for new level
                            if let (
                                Some(basic_stats),
                                Some(character_info),
                                Some(equipment),
                                Some(level),
                                Some(skill_list),
                                Some(status_effects),
                            ) = (
                                character.get::<BasicStats>(),
                                character.get::<CharacterInfo>(),
                                character.get::<Equipment>(),
                                character.get::<Level>(),
                                character.get::<SkillList>(),
                                character.get::<StatusEffects>(),
                            ) {
                                let ability_values = game_data.ability_value_calculator.calculate(
                                    character_info,
                                    level,
                                    equipment,
                                    basic_stats,
                                    skill_list,
                                    status_effects,
                                );

                                if let Some(mut health_points) = character.get_mut::<HealthPoints>()
                                {
                                    health_points.hp = ability_values.get_max_health();
                                }

                                if let Some(mut mana_points) = character.get_mut::<ManaPoints>() {
                                    mana_points.mp = ability_values.get_max_health();
                                }
                            }
                        });
                    });
                }
            }
            Ok(ServerMessage::UpdateSpeed { entity_id, run_speed, passive_attack_speed: _ }) => {
                // TODO: Use passive_attack_speed ?
                if let Some(entity) = client_entity_list.get(entity_id) {
                    commands
                        .entity(entity)
                        .insert(MoveSpeed::new(run_speed as f32));
                }
            }
            Ok(ServerMessage::UpdateSummonPoints {
                used_points,
                max_points,
            }) => {
                if let Some(player_entity) = client_entity_list.player_entity {
                    commands.entity(player_entity).insert(SummonPoints {
                        used_points,
                        max_points,
                    });
                }
            }
            Ok(ServerMessage::UpdateRecoveryRates { hp_bonus, mp_bonus }) => {
                if let Some(player_entity) = client_entity_list.player_entity {
                    commands
                        .entity(player_entity)
                        .insert(RecoveryRateBonus::new(hp_bonus, mp_bonus));
                }
            }
            Ok(ServerMessage::UpdateStatusEffects { entity_id, status_effects: update_status_effects, updated_values }) => {
                if let Some(entity) = client_entity_list.get(entity_id) {
                    commands.add(move |world: &mut World| {
                        let mut entity_mut = world.entity_mut(entity);
                        let mut updated_hp = None;
                        let mut updated_mp = None;

                        if let Some(mut status_effects) = entity_mut.get_mut::<StatusEffects>() {
                            (updated_hp, updated_mp) = apply_status_effect_updates(
                                &mut status_effects,
                                &update_status_effects,
                                &updated_values,
                            );
                        }

                        if let Some(mut status_effects_regen) = entity_mut.get_mut::<StatusEffectsRegen>() {
                            clear_missing_status_effect_regen(
                                &mut status_effects_regen,
                                &update_status_effects,
                            );
                        }

                        if let Some(updated_hp) = updated_hp {
                            if let Some(mut health_points) = entity_mut.get_mut::<HealthPoints>() {
                                health_points.hp = updated_hp;
                            }
                        }

                        if let Some(updated_mp) = updated_mp {
                            if let Some(mut mana_points) = entity_mut.get_mut::<ManaPoints>() {
                                mana_points.mp = updated_mp;
                            }
                        }
                    });
                }
            }
            Ok(ServerMessage::UpdateXpStamina { xp, stamina, source_entity_id: _ }) => {
                if let Some(player_entity) = client_entity_list.player_entity {
                    commands.add(move |world: &mut World| {
                        let mut player = world.entity_mut(player_entity);

                        if let Some(mut player_stamina) = player.get_mut::<Stamina>() {
                            player_stamina.stamina = stamina;
                        }

                        if let Some(mut experience_points) = player.get_mut::<ExperiencePoints>() {
                            let previous_xp = experience_points.xp;
                            experience_points.xp = xp;

                            if xp > previous_xp {
                                world.resource_mut::<Events<ChatboxEvent>>().send(
                                    ChatboxEvent::System(format!(
                                        "You have earned {} experience points.",
                                        xp - previous_xp
                                    )),
                                );
                            }
                        }
                    });
                }
            }
            Ok(ServerMessage::PickupDropItem { drop_entity_id: _, item_slot, item }) => {
                if let Some(player_entity) = client_entity_list.player_entity {
                    if let Some(item_data) =
                        game_data.items.get_base_item(item.get_item_reference())
                    {
                        chatbox_events.send(ChatboxEvent::System(format!(
                            "You have earned {}.",
                            item_data.name
                        )));
                    }
                    ui_sound_events
                        .send(UiSoundEvent::new(SoundId::new(GET_ITEM_SOUND_ID).unwrap()));

                    commands.add(move |world: &mut World| {
                        let mut player = world.entity_mut(player_entity);
                        if let Some(mut inventory) = player.get_mut::<Inventory>() {
                            if let Some(inventory_slot) = inventory.get_item_slot_mut(item_slot)
                            {
                                *inventory_slot = Some(item);
                            }
                        }
                    });
                }
            }
            Ok(ServerMessage::PickupDropMoney { drop_entity_id: _, money }) => {
                if let Some(player_entity) = client_entity_list.player_entity {
                    chatbox_events.send(ChatboxEvent::System(format!(
                        "You have earned {} Zuly.",
                        money.0
                    )));
                    ui_sound_events
                        .send(UiSoundEvent::new(SoundId::new(GET_ITEM_SOUND_ID).unwrap()));

                    commands.add(move |world: &mut World| {
                        let mut player = world.entity_mut(player_entity);
                        if let Some(mut inventory) = player.get_mut::<Inventory>() {
                            inventory.try_add_money(money).ok();
                        }
                    });
                }
            }
            Ok(ServerMessage::PickupDropError { drop_entity_id: _, error }) => match error{
                PickupItemDropError::InventoryFull => {
                    chatbox_events.send(ChatboxEvent::System(
                        "Cannot pickup item, inventory full.".to_string(),
                    ));
                }
                PickupItemDropError::NoPermission => {
                    chatbox_events.send(ChatboxEvent::System(
                        "Cannot pickup item, it does not belong to you.".to_string(),
                    ));
                }
                PickupItemDropError::NotExist => {}
            },
            Ok(ServerMessage::RewardItems { items }) => {
                if let Some(player_entity) = client_entity_list.player_entity {
                    for (_, item) in items.iter() {
                        if let Some(item_data) = item.as_ref().and_then(|item| {
                            game_data.items.get_base_item(item.get_item_reference())
                        }) {
                            chatbox_events.send(ChatboxEvent::System(format!(
                                "You have earned {}.",
                                item_data.name
                            )));
                        }
                    }

                    commands.add(move |world: &mut World| {
                        let mut player = world.entity_mut(player_entity);
                        if let Some(mut inventory) = player.get_mut::<Inventory>() {
                            for (item_slot, item) in items.into_iter() {
                                if let Some(inventory_slot) = inventory.get_item_slot_mut(item_slot)
                                {
                                    *inventory_slot = item;
                                }
                            }
                        }
                    });
                }
            }
            Ok(ServerMessage::RewardMoney { money }) => {
                if let Some(player_entity) = client_entity_list.player_entity {
                    if let Ok(inventory) = query_player_inventory.get_single() {
                        let diff = reward_money_diff(inventory.money, money);
                        if diff > 0 {
                            chatbox_events.send(ChatboxEvent::System(format!(
                                "You have earned {} Zuly.",
                                diff
                            )));
                        } else if diff < 0 {
                            chatbox_events.send(ChatboxEvent::System(format!(
                                "You have lost {} Zuly.",
                                -diff
                            )));
                        }
                    }

                    commands.add(move |world: &mut World| {
                        let mut player = world.entity_mut(player_entity);
                        if let Some(mut inventory) = player.get_mut::<Inventory>() {
                            sync_reward_money(&mut inventory, money);
                        }
                    });
                }
            }
            Ok(ServerMessage::QuestDeleteResult {
                success,
                slot,
                quest_id,
            }) => {
                if success {
                    if let Some(player_entity) = client_entity_list.player_entity {
                        commands.add(move |world: &mut World| {
                            let mut player = world.entity_mut(player_entity);
                            if let Some(mut quest_state) = player.get_mut::<QuestState>() {
                                if let Some(active_quest) = quest_state.active_quests[slot].as_ref()
                                {
                                    if active_quest.quest_id == quest_id {
                                        quest_state.active_quests[slot] = None;
                                    }
                                }
                            }
                        });
                    }
                }
            }
            Ok(ServerMessage::QuestTriggerResult {
                success,
                trigger_hash,
            }) => {
                if success {
                    quest_trigger_events.send(QuestTriggerEvent::ApplyRewards(trigger_hash));
                }
            }
            Ok(ServerMessage::RunNpcDeathTrigger { npc_id }) => {
                if let Some(npc_data) = game_data.npcs.get_npc(npc_id) {
                    quest_trigger_events.send(QuestTriggerEvent::DoTrigger(
                        npc_data.death_quest_trigger_name.as_str().into(),
                    ));
                }
            }
            Ok(ServerMessage::SetHotbarSlot { slot_index, slot }) => {
                if let Some(player_entity) = client_entity_list.player_entity {
                    commands.add(move |world: &mut World| {
                        let mut player = world.entity_mut(player_entity);
                        if let Some(mut hotbar) = player.get_mut::<Hotbar>() {
                            hotbar.set_slot(slot_index, slot);
                        }
                    });
                }
            }
            Ok(ServerMessage::LearnSkillSuccess { skill_slot, skill_id, updated_skill_points }) => {
                if let Some(player_entity) = client_entity_list.player_entity {
                    commands.add(move |world: &mut World| {
                        let mut player = world.entity_mut(player_entity);
                        if let Some(mut skill_list) = player.get_mut::<SkillList>() {
                            if let Some(skill_slot) =
                                skill_list.get_slot_mut(skill_slot)
                            {
                                *skill_slot = skill_id;
                            }
                        }
                    });

                    commands
                        .entity(player_entity)
                        .insert(updated_skill_points);
                }
            }
            Ok(ServerMessage::LearnSkillError { error }) => match error {
                LearnSkillError::AlreadyLearnt => chatbox_events.send(ChatboxEvent::System(
                    "Failed to learn skill, you already know it.".to_string(),
                )),
                LearnSkillError::JobRequirement => chatbox_events.send(ChatboxEvent::System(
                    "Failed to learn skill, you do not satisfy the job requirement.".to_string(),
                )),
                LearnSkillError::SkillRequirement => {
                    chatbox_events.send(ChatboxEvent::System(
                        "Failed to learn skill, you do not satisfy the skill requirement."
                            .to_string(),
                    ))
                }
                LearnSkillError::AbilityRequirement => {
                    chatbox_events.send(ChatboxEvent::System(
                        "Failed to learn skill, you do not satisfy the ability requirement."
                            .to_string(),
                    ))
                }
                LearnSkillError::Full => chatbox_events.send(ChatboxEvent::System(
                    "Failed to learn skill, you have too many skills.".to_string(),
                )),
                LearnSkillError::InvalidSkillId => chatbox_events.send(ChatboxEvent::System(
                    "Failed to learn skill, invalid skill.".to_string(),
                )),
                LearnSkillError::SkillPointRequirement => {
                    chatbox_events.send(ChatboxEvent::System(
                        "Failed to learn skill, not enough skill points.".to_string(),
                    ))
                }
            },
            Ok(ServerMessage::LevelUpSkillSuccess { skill_slot, skill_id, skill_points }) => {
                if let Some(player_entity) = client_entity_list.player_entity {
                    commands.add(move |world: &mut World| {
                        let mut player = world.entity_mut(player_entity);
                        player.insert(skill_points);

                        if let Some(mut skill_list) = player.get_mut::<SkillList>() {
                            if let Some(skill_slot) = skill_list.get_slot_mut(skill_slot) {
                                *skill_slot = Some(skill_id);
                            }
                        }
                    });
                }
            }
            Ok(ServerMessage::LevelUpSkillError { error, skill_points }) => {
                match error {
                    LevelUpSkillError::Failed => chatbox_events.send(ChatboxEvent::System(
                        "Failed to level up skill.".to_string(),
                    )),
                    LevelUpSkillError::JobRequirement => {
                        chatbox_events.send(ChatboxEvent::System(
                            "Failed to level up skill, you do not satisfy the job requirement."
                                .to_string(),
                        ))
                    }
                    LevelUpSkillError::SkillRequirement => {
                        chatbox_events.send(ChatboxEvent::System(
                            "Failed to level up skill, you do not satisfy the skill requirement."
                                .to_string(),
                        ))
                    }
                    LevelUpSkillError::AbilityRequirement => {
                        chatbox_events.send(ChatboxEvent::System(
                            "Failed to level up skill, you do not satisfy the ability requirement."
                                .to_string(),
                        ))
                    }
                    LevelUpSkillError::MoneyRequirement => {
                        chatbox_events.send(ChatboxEvent::System(
                            "Failed to level up skill, not enough money.".to_string(),
                        ))
                    }
                    LevelUpSkillError::SkillPointRequirement => {
                        chatbox_events.send(ChatboxEvent::System(
                            "Failed to level up skill, not enough skill points.".to_string(),
                        ))
                    }
                }

                if let Some(player_entity) = client_entity_list.player_entity {
                    commands
                        .entity(player_entity)
                        .insert(skill_points);
                }
            }
            Ok(ServerMessage::UseEmote { entity_id, motion_id, is_stop }) => {
                if let Some(entity) = client_entity_list.get(entity_id) {
                    let new_command = NextCommand::with_emote(motion_id, is_stop);
                    commands.entity(entity).insert(new_command);
                }
            }
            Ok(ServerMessage::SitToggle { entity_id }) => {
                if let Some(entity) = client_entity_list.get(entity_id) {
                    commands.add(move |world: &mut World| {
                        let mut character = world.entity_mut(entity);
                        let is_sitting =
                            matches!(character.get::<Command>(), Some(Command::Sit(_)));

                        if let Some(mut next_command) = character.get_mut::<NextCommand>() {
                            if is_sitting {
                                // If next command is already set then the command system will make the
                                // entity stand up before performing next command. So we only need to
                                // explicitly start to stand up if next command is not set.
                                if next_command.is_none() {
                                    *next_command = NextCommand::with_standing();
                                }
                            } else {
                                *next_command = NextCommand::with_sitting();
                            }
                        }
                    });
                }
            }
            Ok(ServerMessage::UseItem { entity_id, item }) => {
                if let Some(entity) = client_entity_list.get(entity_id) {
                    use_item_events.send(UseItemEvent { entity, item });
                }
            }
            Ok(ServerMessage::CastSkillSelf { entity_id, skill_id, cast_motion_id }) => {
                if let Some(entity) = client_entity_list.get(entity_id) {
                    commands.entity(entity).insert(NextCommand::with_cast_skill(
                        skill_id,
                        None,
                        cast_motion_id,
                        None,
                        None,
                    ));
                    queue_bonfire_cast_sound(
                        &mut commands,
                        &game_data,
                        &asset_server,
                        &sound_cache,
                        &sound_settings,
                        &query_global_transform,
                        client_entity_list.player_entity,
                        entity,
                        skill_id,
                    );

                    if client_entity_list.player_entity == Some(entity) {
                        if let Some(skill_data) = game_data.skills.get_skill(skill_id) {
                            match skill_data.cooldown {
                                SkillCooldown::Skill { duration } => {
                                    commands.add(move |world: &mut World| {
                                        let mut character = world.entity_mut(entity);

                                        if let Some(mut cooldowns) = character.get_mut::<Cooldowns>() {
                                            cooldowns.set_skill_cooldown(skill_id, duration);
                                        }
                                    });
                                },
                                SkillCooldown::Group { group, duration } => {
                                    commands.add(move |world: &mut World| {
                                        let mut character = world.entity_mut(entity);

                                        if let Some(mut cooldowns) = character.get_mut::<Cooldowns>() {
                                            cooldowns.set_skill_group_cooldown(group.get(), duration);
                                        }
                                    });
                                },
                            }
                        }
                    }
                }
            }
            Ok(ServerMessage::CastSkillTargetEntity { entity_id, skill_id, target_entity_id, target_distance: _, target_position: _, cast_motion_id }) => {
                if let Some(entity) = client_entity_list.get(entity_id) {
                    if let Some(target_entity) = client_entity_list.get(target_entity_id) {
                        commands.entity(entity).insert(NextCommand::with_cast_skill(
                            skill_id,
                            Some(CommandCastSkillTarget::Entity(target_entity)),
                            cast_motion_id,
                            None,
                            None,
                        ));
                    }
                    queue_bonfire_cast_sound(
                        &mut commands,
                        &game_data,
                        &asset_server,
                        &sound_cache,
                        &sound_settings,
                        &query_global_transform,
                        client_entity_list.player_entity,
                        entity,
                        skill_id,
                    );

                    if client_entity_list.player_entity == Some(entity) {
                        if let Some(skill_data) = game_data.skills.get_skill(skill_id) {
                            match skill_data.cooldown {
                                SkillCooldown::Skill { duration } => {
                                    commands.add(move |world: &mut World| {
                                        let mut character = world.entity_mut(entity);

                                        if let Some(mut cooldowns) = character.get_mut::<Cooldowns>() {
                                            cooldowns.set_skill_cooldown(skill_id, duration);
                                        }
                                    });
                                },
                                SkillCooldown::Group { group, duration } => {
                                    commands.add(move |world: &mut World| {
                                        let mut character = world.entity_mut(entity);

                                        if let Some(mut cooldowns) = character.get_mut::<Cooldowns>() {
                                            cooldowns.set_skill_group_cooldown(group.get(), duration);
                                        }
                                    });
                                },
                            }
                        }
                    }
                }
            }
            Ok(ServerMessage::CastSkillTargetPosition { entity_id, skill_id, target_position, cast_motion_id }) => {
                if let Some(entity) = client_entity_list.get(entity_id) {
                    commands.entity(entity).insert(NextCommand::with_cast_skill(
                        skill_id,
                        Some(CommandCastSkillTarget::Position(target_position)),
                        cast_motion_id,
                        None,
                        None,
                    ));
                    queue_bonfire_cast_sound(
                        &mut commands,
                        &game_data,
                        &asset_server,
                        &sound_cache,
                        &sound_settings,
                        &query_global_transform,
                        client_entity_list.player_entity,
                        entity,
                        skill_id,
                    );

                    if client_entity_list.player_entity == Some(entity) {
                        if let Some(skill_data) = game_data.skills.get_skill(skill_id) {
                            match skill_data.cooldown {
                                SkillCooldown::Skill { duration } => {
                                    commands.add(move |world: &mut World| {
                                        let mut character = world.entity_mut(entity);

                                        if let Some(mut cooldowns) = character.get_mut::<Cooldowns>() {
                                            cooldowns.set_skill_cooldown(skill_id, duration);
                                        }
                                    });
                                },
                                SkillCooldown::Group { group, duration } => {
                                    commands.add(move |world: &mut World| {
                                        let mut character = world.entity_mut(entity);

                                        if let Some(mut cooldowns) = character.get_mut::<Cooldowns>() {
                                            cooldowns.set_skill_group_cooldown(group.get(), duration);
                                        }
                                    });
                                },
                            }
                        }
                    }
                }
            }
            Ok(ServerMessage::CancelCastingSkill { entity_id, reason: _ }) => {
                if let Some(entity) = client_entity_list.get(entity_id) {
                    commands.add(move |world: &mut World| {
                        let mut character = world.entity_mut(entity);

                        if let Some(mut command) = character.get_mut::<Command>() {
                            if matches!(*command, Command::CastSkill(_)) {
                                *command = Command::with_stop();
                            }
                        }
                    });
                }
            }
            Ok(ServerMessage::StartCastingSkill { entity_id: _ }) => {
                // Nah bruv
            }
            Ok(ServerMessage::FinishCastingSkill { entity_id, skill_id }) => {
                if let Some(entity) = client_entity_list.get(entity_id) {
                    commands.add(move |world: &mut World| {
                        let mut character = world.entity_mut(entity);

                        if let Some(mut command) = character.get_mut::<Command>() {
                            if let Command::CastSkill(command_cast_skill) = command.as_mut() {
                                if command_cast_skill.skill_id == skill_id {
                                    command_cast_skill.ready_action = true;
                                    return;
                                }
                             }
                        }

                        if let Some(mut next_command) = character.get_mut::<NextCommand>() {
                            if let Some(Command::CastSkill(command_cast_skill)) = (*next_command).as_mut() {
                                if command_cast_skill.skill_id == skill_id {
                                    command_cast_skill.ready_action = true;
                                    return;
                                }
                            }
                        }

                        if let Some(command) = character.get::<Command>() {
                            if let Some(next_command) = character.get::<NextCommand>() {
                                log::error!("FinishCastingSkill entity was not in expected state, command: {:?}, next command: {:?}, expected CastSkill({:?})", *command, *next_command, skill_id);
                            }
                        }
                    });

                    if let Some(use_ability) = game_data
                        .skills
                        .get_skill(skill_id)
                        .map(|skill_data| skill_data.use_ability.clone())
                    {
                        let is_player = client_entity_list.player_entity == Some(entity);
                        commands.add(move |world: &mut World| {
                            let mut target = world.entity_mut(entity);

                            for (use_ability_type, mut use_ability_value) in use_ability {
                                // We only apply health point modification to other entities
                                if !is_player && use_ability_type != AbilityType::Health {
                                    continue;
                                }

                                if use_ability_type == AbilityType::Mana {
                                    if let Some(ability_values) = target.get::<AbilityValues>() {
                                        let use_mana_rate =
                                            (100 - ability_values.get_save_mana()) as f32 / 100.0;

                                        use_ability_value =
                                            (use_ability_value as f32 * use_mana_rate) as i32;
                                    }
                                }

                                ability_values_add_value_exclusive(
                                    use_ability_type,
                                    -use_ability_value,
                                    &mut target,
                                );
                            }
                        });
                    }
                }
            }
            Ok(ServerMessage::ApplySkillEffect { entity_id, caster_entity_id, caster_intelligence, skill_id, effect_success }) => {
                if let Some(defender_entity) = client_entity_list.get(entity_id) {
                    let caster_entity = client_entity_list.get(caster_entity_id);

                    commands.add(move |world: &mut World| {
                        let mut defender = world.entity_mut(defender_entity);

                        if let Some(mut pending_skill_effect_list) =
                            defender.get_mut::<PendingSkillEffectList>()
                        {
                            pending_skill_effect_list.push(PendingSkillEffect::new(
                                skill_id,
                                caster_entity,
                                caster_intelligence,
                                effect_success,
                            ));
                        }

                        if let Some(caster_entity) = caster_entity {
                            if let Some(mut pending_skill_target_list) = world
                                .entity_mut(caster_entity)
                                .get_mut::<PendingSkillTargetList>()
                            {
                                pending_skill_target_list.push(PendingSkillTarget::new(
                                    skill_id,
                                    defender_entity,
                                ));
                            }
                        }
                    });
                }
            }
            Ok(ServerMessage::NpcStoreTransactionError { error }) => {
                let message = match error {
                    rose_game_common::messages::server::NpcStoreTransactionError::NotSameUnion => {
                        "You cannot use this union store.".to_string()
                    }
                    rose_game_common::messages::server::NpcStoreTransactionError::NotEnoughUnionPoints => {
                        "You do not have enough Union Points.".to_string()
                    }
                    _ => format!("Store transation failed with error {:?}", error),
                };
                chatbox_events.send(ChatboxEvent::System(message));
            }
            Ok(ServerMessage::PartyCreate { entity_id }) => {
                if let Some(inviter_entity) = client_entity_list.get(entity_id) {
                    party_events.send(PartyEvent::InvitedCreate(inviter_entity));
                }
            }
            Ok(ServerMessage::PartyInvite { entity_id }) => {
                if let Some(inviter_entity) = client_entity_list.get(entity_id) {
                    party_events.send(PartyEvent::InvitedJoin(inviter_entity));
                }
            }
            Ok(ServerMessage::PartyAcceptCreate { entity_id }) => {
                if let Some(invited_entity) = client_entity_list.get(entity_id) {
                    commands.add(move |world: &mut World| {
                        if let Some(invited_entity_name) =
                            world.entity(invited_entity).get::<ClientEntityName>()
                        {
                            let message = format!(
                                "{} accepted your party invite.",
                                invited_entity_name.as_str()
                            );
                            world
                                .resource_mut::<Events<ChatboxEvent>>()
                                .send(ChatboxEvent::System(message));
                        }
                    });
                }

                if let Some(player_entity) = client_entity_list.player_entity {
                    commands.entity(player_entity).insert(PartyInfo {
                        owner: PartyOwner::Player,
                        ..Default::default()
                    });
                }
            }
            Ok(ServerMessage::PartyAcceptInvite { .. }) => {
                if let Some(player_entity) = client_entity_list.player_entity {
                    commands.entity(player_entity).insert(PartyInfo {
                        owner: PartyOwner::Unknown,
                        ..Default::default()
                    });

                    commands.add(move |world: &mut World| {
                        if let Some(player_entity_name) =
                            world.entity(player_entity).get::<ClientEntityName>()
                        {
                            let message =
                                format!("{} has joined the party.", player_entity_name.as_str());
                            world
                                .resource_mut::<Events<ChatboxEvent>>()
                                .send(ChatboxEvent::System(message));
                        }
                    });
                }
            }
            Ok(ServerMessage::PartyRejectInvite { reason: _, entity_id }) => {
                if let Some(invited_entity) = client_entity_list.get(entity_id) {
                    commands.add(move |world: &mut World| {
                        if let Some(invited_entity_name) =
                            world.entity(invited_entity).get::<ClientEntityName>()
                        {
                            let message = format!(
                                "{} rejected your party invite.",
                                invited_entity_name.as_str()
                            );
                            world
                                .resource_mut::<Events<ChatboxEvent>>()
                                .send(ChatboxEvent::System(message));
                        }
                    });
                }
            }
            Ok(ServerMessage::PartyChangeOwner { entity_id }) => {
                if let Some(player_entity) = client_entity_list.player_entity {
                    let is_player_owner =
                        Some(entity_id) == client_entity_list.player_entity_id;

                    commands.add(move |world: &mut World| {
                        if let Some(mut party_info) =
                            world.entity_mut(player_entity).get_mut::<PartyInfo>()
                        {
                            if is_player_owner {
                                party_info.owner = PartyOwner::Player;

                                if let Some(character_info) =
                                    world.entity(player_entity).get::<CharacterInfo>()
                                {
                                    let message = format!(
                                        "{} is now leader of the party.",
                                        &character_info.name
                                    );
                                    world
                                        .resource_mut::<Events<ChatboxEvent>>()
                                        .send(ChatboxEvent::System(message));
                                }
                            } else {
                                party_info.owner = PartyOwner::Unknown;

                                for member in party_info.members.iter() {
                                    if let PartyMemberInfo::Online(member_info_online) = member {
                                        if member_info_online.entity_id == entity_id {
                                            let message = format!(
                                                "{} is now leader of the party.",
                                                &member_info_online.name
                                            );

                                            party_info.owner = PartyOwner::Character(
                                                member_info_online.character_id,
                                            );

                                            world
                                                .resource_mut::<Events<ChatboxEvent>>()
                                                .send(ChatboxEvent::System(message));
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                    });
                }
            }
            Ok(ServerMessage::PartyDelete) => {
                if let Some(player_entity) = client_entity_list.player_entity {
                    commands.entity(player_entity).remove::<PartyInfo>();
                    chatbox_events.send(ChatboxEvent::System("You have left the party.".into()));
                }
            }
            Ok(ServerMessage::PartyMemberList {
                mut members,
                item_sharing,
                xp_sharing,
                ..
            }) => {
                if let Some(player_entity) = client_entity_list.player_entity {
                    commands.add(move |world: &mut World| {
                        let mut player = world.entity_mut(player_entity);

                        if !player.contains::<PartyInfo>() {
                            player.insert(PartyInfo {
                                item_sharing,
                                xp_sharing,
                                ..Default::default()
                            });
                        }

                        let mut party_info = player.get_mut::<PartyInfo>().unwrap();
                        if matches!(party_info.owner, PartyOwner::Unknown) {
                            party_info.owner = PartyOwner::Character(members[0].get_character_id());
                        }

                        let mut messages: ArrayVec<String, 10> = ArrayVec::new();
                        for member in members.iter() {
                            messages.push(format!("{} has joined the party.", member.get_name()));
                        }

                        party_info.members.append(&mut members);

                        let mut chatbox_events = world.resource_mut::<Events<ChatboxEvent>>();
                        for message in messages {
                            chatbox_events.send(ChatboxEvent::System(message));
                        }
                    });
                }
            }
            Ok(ServerMessage::PartyMemberLeave {
                leaver_character_id,
                owner_character_id,
            }) => {
                if let Some(player_entity) = client_entity_list.player_entity {
                    commands.add(move |world: &mut World| {
                        let mut player = world.entity_mut(player_entity);
                        let player_unique_id =
                            player.get::<CharacterInfo>().map(|info| info.unique_id);

                        if let Some(mut party_info) = player.get_mut::<PartyInfo>() {
                            if player_unique_id == Some(owner_character_id) {
                                party_info.owner = PartyOwner::Player;
                            } else {
                                party_info.owner = PartyOwner::Character(owner_character_id);
                            }

                            if let Some(index) = party_info
                                .members
                                .iter()
                                .position(|x| x.get_character_id() == leaver_character_id)
                            {
                                let message = format!(
                                    "{} has left the party.",
                                    party_info.members[index].get_name()
                                );

                                party_info.members.remove(index);

                                world
                                    .resource_mut::<Events<ChatboxEvent>>()
                                    .send(ChatboxEvent::System(message));
                            }
                        }
                    });
                }
            }
            Ok(ServerMessage::PartyMemberDisconnect { character_id }) => {
                if let Some(player_entity) = client_entity_list.player_entity {
                    commands.add(move |world: &mut World| {
                        if let Some(mut party_info) =
                            world.entity_mut(player_entity).get_mut::<PartyInfo>()
                        {
                            if let Some(party_member) = party_info
                                .members
                                .iter_mut()
                                .find(|x| x.get_character_id() == character_id)
                            {
                                if let PartyMemberInfo::Online(party_member_online) = party_member {
                                    let message =
                                        format!("{} has disconnected.", &party_member_online.name);

                                    *party_member =
                                        PartyMemberInfo::Offline(PartyMemberInfoOffline {
                                            character_id: party_member_online.character_id,
                                            name: party_member_online.name.clone(),
                                        });

                                    world
                                        .resource_mut::<Events<ChatboxEvent>>()
                                        .send(ChatboxEvent::System(message));
                                }
                            }
                        }
                    });
                }
            }
            Ok(ServerMessage::PartyMemberKicked { character_id }) => {
                if let Some(player_entity) = client_entity_list.player_entity {
                    commands.add(move |world: &mut World| {
                        if let Some(mut party_info) =
                            world.entity_mut(player_entity).get_mut::<PartyInfo>()
                        {
                            if let Some(index) = party_info
                                .members
                                .iter()
                                .position(|x| x.get_character_id() == character_id)
                            {
                                let message = format!(
                                    "{} has been kicked from the party.",
                                    party_info.members[index].get_name()
                                );
                                party_info.members.remove(index);

                                world
                                    .resource_mut::<Events<ChatboxEvent>>()
                                    .send(ChatboxEvent::System(message));
                            }
                        }
                    });
                }
            }
            Ok(ServerMessage::PartyMemberUpdateInfo { member_info }) => {
                let member_entity = client_entity_list.get(member_info.entity_id);
                let player_entity = client_entity_list.player_entity;

                if member_entity.is_some() || player_entity.is_some() {
                    commands.add(move |world: &mut World| {
                        if let Some(mut member) = member_entity
                            .and_then(|member_entity| world.get_entity_mut(member_entity))
                        {
                            if let Some(mut basic_stats) = member.get_mut::<BasicStats>() {
                                basic_stats.concentration = member_info.concentration;
                            }

                            if let Some(mut health_points) = member.get_mut::<HealthPoints>() {
                                health_points.hp = member_info.health_points.hp;
                            }
                        }

                        if let Some(mut player) = player_entity
                            .and_then(|player_entity| world.get_entity_mut(player_entity))
                        {
                            if let Some(mut party_info) = player.get_mut::<PartyInfo>() {
                                if let Some(party_member) =
                                    party_info.members.iter_mut().find(|x| {
                                        x.get_character_id() == member_info.character_id
                                    })
                                {
                                    *party_member = PartyMemberInfo::Online(member_info);
                                }
                            }
                        }
                    });
                }
            }
            Ok(ServerMessage::PartyMemberRewardItem {
                client_entity_id,
                item,
            }) => {
                let member_entity = client_entity_list.get(client_entity_id);
                let item_name = game_data
                    .items
                    .get_base_item(item.get_item_reference())
                    .map(|item_data| item_data.name);

                if let (Some(member_entity), Some(item_name)) = (member_entity, item_name) {
                    commands.add(move |world: &mut World| {
                        if let Some(member) = world.get_entity(member_entity) {
                            if let Some(member_entity_name) = member.get::<ClientEntityName>() {
                                let chat_message = format!(
                                    "{} has earned {}.",
                                    member_entity_name.as_str(),
                                    item_name
                                );
                                world
                                    .resource_mut::<Events<ChatboxEvent>>()
                                    .send(ChatboxEvent::System(chat_message));
                            }
                        }
                    });
                }
            }
            Ok(ServerMessage::PartyUpdateRules { item_sharing, xp_sharing }) => {
                if let Some(player_entity) = client_entity_list.player_entity {
                    commands.add(move |world: &mut World| {
                        if let Some(mut party_info) =
                            world.entity_mut(player_entity).get_mut::<PartyInfo>()
                        {
                            party_info.item_sharing = item_sharing;
                            party_info.xp_sharing = xp_sharing;

                            let mut chatbox_events = world.resource_mut::<Events<ChatboxEvent>>();
                            chatbox_events
                                .send(ChatboxEvent::System("Party rules have changed.".into()));
                            chatbox_events.send(ChatboxEvent::System(format!(
                                "Experience points sharing: {}.",
                                match xp_sharing {
                                    PartyXpSharing::EqualShare => "Equal Share",
                                    PartyXpSharing::DistributedByLevel => "Distributed by Level",
                                }
                            )));
                            chatbox_events.send(ChatboxEvent::System(format!(
                                "Item sharing: {}.",
                                match item_sharing {
                                    PartyItemSharing::EqualLootDistribution =>
                                        "Equal Loot Distribution",
                                    PartyItemSharing::AcquisitionOrder => "Acquisition Order",
                                }
                            )));
                        }
                    });
                }
            }
            Ok(ServerMessage::UpdateSkillList { skill_list: update_skills }) => {
                if let Some(player_entity) = client_entity_list.player_entity {
                    commands.add(move |world: &mut World| {
                        let mut player = world.entity_mut(player_entity);
                        if let Some(mut skill_list) = player.get_mut::<SkillList>() {
                            for update_skill in update_skills {
                                if let Some(skill_slot) =
                                    skill_list.get_slot_mut(update_skill.skill_slot)
                                {
                                    *skill_slot = update_skill.skill_id;
                                }
                            }
                        }
                    });
                }
            }
            Ok(ServerMessage::PartyLevelXp { level, xp, is_level_up }) => {
                if let Some(player_entity) = client_entity_list.player_entity {
                    if let Some(event) = party_level_up_event(player_entity, is_level_up) {
                        // Play level-up effect and sound on the player
                        client_entity_events.send(event);

                        // Restore player HP/MP to full and update party info
                        commands.add(move |world: &mut World| {
                            let mut player = world.entity_mut(player_entity);

                            if let Some(mut party_info) = player.get_mut::<PartyInfo>() {
                                party_info.level = level as i32;
                                party_info.experience = xp as i32;
                            }

                            let max_hp = player.get::<AbilityValues>().map(|av| av.get_max_health());
                            let max_mp = player.get::<AbilityValues>().map(|av| av.get_max_mana());

                            if let (Some(max_hp), Some(mut health_points)) = (max_hp, player.get_mut::<HealthPoints>()) {
                                health_points.hp = max_hp;
                            }
                            if let (Some(max_mp), Some(mut mana_points)) = (max_mp, player.get_mut::<ManaPoints>()) {
                                mana_points.mp = max_mp;
                            }
                        });
                    } else {
                        commands.add(move |world: &mut World| {
                            if let Some(mut party_info) =
                                world.entity_mut(player_entity).get_mut::<PartyInfo>()
                            {
                                party_info.level = level as i32;
                                party_info.experience = xp as i32;
                            }
                        });
                    }
                }
            }
            Ok(ServerMessage::OpenPersonalStore {
                entity_id,
                skin,
                title,
            }) => {
                if let Some(entity) = client_entity_list.get(entity_id) {
                    commands.entity(entity).insert(PersonalStore {
                        title,
                        skin: skin as usize,
                    });
                }
            }
            Ok(ServerMessage::ClosePersonalStore { entity_id }) => {
                let target_entity = client_entity_list
                    .get(entity_id)
                    .or_else(|| {
                        if client_entity_list.player_entity_id == Some(entity_id) {
                            client_entity_list.player_entity
                        } else {
                            None
                        }
                    });

                if let Some(entity) = target_entity {
                    log::info!(
                        "personal-store: close received entity_id={} entity={:?}",
                        entity_id.0,
                        entity
                    );
                    let mut entity_commands = commands.entity(entity);
                    entity_commands.remove::<PersonalStore>();

                    // Ensure local command state exits PersonalStore immediately so movement
                    // can resume even before further server movement updates arrive.
                    entity_commands.insert(Command::with_stop());
                    entity_commands.insert(NextCommand::with_stop());
                } else {
                    log::warn!(
                        "personal-store: close received for unknown entity_id={}",
                        entity_id.0
                    );
                }
            }
            Ok(ServerMessage::PersonalStoreItemList { sell_items, buy_items  }) => {
                personal_store_events.send(PersonalStoreEvent::SetItemList {
                    sell_items,
                    buy_items,
                });
            }
            Ok(ServerMessage::PersonalStoreTransaction {
                status,
                store_entity_id,
                update_store,
            }) => {
                if !update_store.is_empty() {
                    if let Some(entity) = client_entity_list.get(store_entity_id) {
                        match status {
                            PersonalStoreTransactionStatus::Cancelled => {}
                            PersonalStoreTransactionStatus::SoldOut
                            | PersonalStoreTransactionStatus::BoughtFromStore => {
                                personal_store_events.send(PersonalStoreEvent::UpdateSellList {
                                    entity,
                                    item_list: update_store,
                                });
                            }
                            PersonalStoreTransactionStatus::NoMoreNeed
                            | PersonalStoreTransactionStatus::SoldToStore => {
                                personal_store_events.send(PersonalStoreEvent::UpdateBuyList {
                                    entity,
                                    item_list: update_store,
                                });
                            }
                        }
                    }
                }

                match status {
                    PersonalStoreTransactionStatus::Cancelled => {
                        chatbox_events
                            .send(ChatboxEvent::System("Transaction failed.".to_string()));
                    }
                    PersonalStoreTransactionStatus::SoldOut => {
                        chatbox_events.send(ChatboxEvent::System(
                            "Transaction failed. Item has sold out.".to_string(),
                        ));
                    }
                    PersonalStoreTransactionStatus::NoMoreNeed => {
                        chatbox_events.send(ChatboxEvent::System(
                            "Transaction failed. Item is no longer wanted.".to_string(),
                        ));
                    }
                    _ => {}
                }
            }
            Ok(ServerMessage::PersonalStoreTransactionUpdateInventory { money, items }) => {
                if let Some(player_entity) = client_entity_list.player_entity {
                    commands.add(move |world: &mut World| {
                        let player = world.entity(player_entity);

                        if let Some(inventory) = player.get::<Inventory>() {
                            let transaction_price = money.0 - inventory.money.0;

                            if let Some((item_slot, transaction_item)) = items.first() {
                                let transaction_item = transaction_item.as_ref();
                                let inventory_item = inventory.get_item(*item_slot);
                                let (transaction_quantity, transaction_item) =
                                    match (transaction_item, inventory_item) {
                                        (Some(transaction_item), Some(inventory_item)) => (
                                            transaction_item.get_quantity() as i32
                                                - inventory_item.get_quantity() as i32,
                                            Some(inventory_item.get_item_reference()),
                                        ),
                                        (None, Some(inventory_item)) => (
                                            inventory_item.get_quantity() as i32,
                                            Some(inventory_item.get_item_reference()),
                                        ),
                                        (Some(transaction_item), None) => (
                                            transaction_item.get_quantity() as i32,
                                            Some(transaction_item.get_item_reference()),
                                        ),
                                        (None, None) => (0, None),
                                    };

                                let game_data = world.resource::<GameData>();
                                if let Some(item_data) = transaction_item
                                    .and_then(|item| game_data.items.get_base_item(item))
                                {
                                    let message = if transaction_quantity > 1 {
                                        format!(
                                            "You have {} {}x {} for {} Zuly.",
                                            if transaction_price < 0 {
                                                "purchased"
                                            } else {
                                                "sold"
                                            },
                                            transaction_quantity,
                                            item_data.name,
                                            transaction_price.abs()
                                        )
                                    } else {
                                        format!(
                                            "You have {} {} for {} Zuly.",
                                            if transaction_price < 0 {
                                                "purchased"
                                            } else {
                                                "sold"
                                            },
                                            item_data.name,
                                            transaction_price.abs()
                                        )
                                    };
                                    let mut chatbox_events =
                                        world.resource_mut::<Events<ChatboxEvent>>();
                                    chatbox_events.send(ChatboxEvent::System(message));
                                }
                            }
                        }

                        update_inventory_and_money(world, player_entity, items, Some(money));
                    });
                }
            }
            Ok(ServerMessage::BankOpen) => {
                commands.add(move |world: &mut World| {
                    let mut chatbox_events = world.resource_mut::<Events<BankEvent>>();
                    chatbox_events.send(BankEvent::Show);
                });
            }
            Ok(ServerMessage::BankSetItems { items }) => {
                if let Some(player_entity) = client_entity_list.player_entity {
                    let mut slots = vec![None; 160];

                    for (bank_slot_index, item) in items {
                        let bank_slot_index = bank_slot_index as usize;
                        if bank_slot_index > slots.len() {
                            slots.resize(bank_slot_index + 1, None);
                        }
                        slots[bank_slot_index] = item;
                    }

                    commands.add(move |world: &mut World| {
                        world.entity_mut(player_entity).insert(Bank { slots });

                        let mut chatbox_events = world.resource_mut::<Events<BankEvent>>();
                        chatbox_events.send(BankEvent::Show);
                    });
                }
            }
            Ok(ServerMessage::BankUpdateItems { items }) => {
                if let Some(player_entity) = client_entity_list.player_entity {
                    commands.add(move |world: &mut World| {
                        if let Some(mut bank) = world.entity_mut(player_entity).get_mut::<Bank>() {
                            for (bank_slot_index, item) in items {
                                let bank_slot_index = bank_slot_index as usize;

                                if bank_slot_index > bank.slots.len() {
                                    bank.slots.resize(bank_slot_index + 1, None);
                                }

                                bank.slots[bank_slot_index] = item;
                            }
                        }
                    });
                }
            }
            Ok(ServerMessage::BankTransaction {
                inventory_item_slot,
                inventory_item,
                inventory_money,
                bank_slot,
                bank_item,
            }) => {
                if let Some(player_entity) = client_entity_list.player_entity {
                    commands.add(move |world: &mut World| {
                        if let Some(mut inventory) =
                            world.entity_mut(player_entity).get_mut::<Inventory>()
                        {
                            if let Some(item_slot) =
                                inventory.get_item_slot_mut(inventory_item_slot)
                            {
                                *item_slot = inventory_item;
                            }

                            if let Some(inventory_money) = inventory_money {
                                inventory.money = inventory_money;
                            }
                        }

                        if let Some(mut bank) = world.entity_mut(player_entity).get_mut::<Bank>() {
                            if let Some(bank_slot) = bank.slots.get_mut(bank_slot) {
                                *bank_slot = bank_item;
                            }
                        }
                    });
                }
            }
            Ok(ServerMessage::MoveToggle {
                entity_id,
                move_mode,
                .. // TODO: run_speed
            }) => {
                if let Some(entity) = client_entity_list.get(entity_id) {
                    commands.entity(entity).insert(move_mode);
                }
            }
            Ok(ServerMessage::ChangeNpcId { entity_id, npc_id }) => {
                if let Some(entity) = client_entity_list.get(entity_id) {
                    commands.add(move |world: &mut World| {
                        let mut entity_mut = world.entity_mut(entity);
                        if let Some(mut npc) = entity_mut.get_mut::<Npc>() {
                            npc.id = npc_id;
                        }
                    });
                }
            }
            Ok(ServerMessage::ClanInfo { id, mark, level, points, money, name, description, position, contribution, skills }) => {
                if let Some(player_entity) = client_entity_list.player_entity {
                    commands.entity(player_entity).insert((
                        Clan {
                            unique_id: id,
                            name: name.clone(),
                            description,
                            mark,
                            money,
                            points,
                            level,
                            members: Vec::new(),
                            skills,
                        },
                        ClanMembership {
                            clan_unique_id: id,
                            mark,
                            level,
                            name,
                            position,
                            contribution,
                        }));
                }
            }
            Ok(ServerMessage::ClanUpdateInfo {
                id,
                mark,
                level,
                points,
                money,
                description,
                skills,
            }) => {
                if let Some(player_entity) = client_entity_list.player_entity {
                    commands.add(move |world: &mut World| {
                        let mut entity_mut = world.entity_mut(player_entity);
                        if let Some(mut clan) = entity_mut.get_mut::<Clan>() {
                            clan.unique_id = id;
                            clan.mark = mark;
                            clan.level = level;
                            clan.points = points;
                            clan.money = money;
                            clan.description = description;
                            clan.skills = skills;
                        }
                        if let Some(mut clan_membership) = entity_mut.get_mut::<ClanMembership>() {
                            clan_membership.clan_unique_id = id;
                            clan_membership.mark = mark;
                            clan_membership.level = level;
                        }
                    });
                }
            }
            Ok(ServerMessage::CharacterUpdateClan { client_entity_id, id, name, mark, level, position  }) => {
                if let Some(entity) = client_entity_list.get(client_entity_id) {
                    commands.entity(entity).insert(
                        ClanMembership {
                            clan_unique_id: id,
                            mark,
                            level,
                            name,
                            position,
                            contribution: ClanPoints(0),
                        });
                }
            }
            Ok(ServerMessage::ClanMemberConnected { name, channel_id  }) =>  {
                if let Some(player_entity) = client_entity_list.player_entity {
                    commands.add(move |world: &mut World| {
                        let mut entity_mut = world.entity_mut(player_entity);
                        if let Some(mut clan) = entity_mut.get_mut::<Clan>() {
                            if let Some(member) = clan.find_member_mut(&name) {
                                member.channel_id = Some(channel_id);
                            }
                        }
                    });
                }
            }
            Ok(ServerMessage::ClanMemberDisconnected { name  }) =>  {
                if let Some(player_entity) = client_entity_list.player_entity {
                    commands.add(move |world: &mut World| {
                        let mut entity_mut = world.entity_mut(player_entity);
                        if let Some(mut clan) = entity_mut.get_mut::<Clan>() {
                            if let Some(member) = clan.find_member_mut(&name) {
                                member.channel_id = None;
                            }
                        }
                    });
                }
            }
            Ok(ServerMessage::ClanCreateError { error }) =>  {
                match error {
                    ClanCreateError::Failed => {
                        message_box_events.send(MessageBoxEvent::Show { message: game_data.client_strings.clan_create_error.into(), modal: false, ok: None, cancel: None });
                    },
                    ClanCreateError::NameExists => {
                        message_box_events.send(MessageBoxEvent::Show { message: game_data.client_strings.clan_create_error_name.into(), modal: false, ok: None, cancel: None });
                    },
                    ClanCreateError::NoPermission => {
                        message_box_events.send(MessageBoxEvent::Show { message: game_data.client_strings.clan_create_error_permission.into(), modal: false, ok: None, cancel: None });
                    },
                    ClanCreateError::UnmetCondition => {
                        message_box_events.send(MessageBoxEvent::Show { message: game_data.client_strings.clan_create_error_condition.into(), modal: false, ok: None, cancel: None });
                    },
                }
            }
            Ok(ServerMessage::ClanUpgradeResult { result }) => {
                let message = match result {
                    ClanUpgradeResult::Success => "Clan grade upgraded successfully.",
                    ClanUpgradeResult::NoClan => "You are not in a clan.",
                    ClanUpgradeResult::NoPermission => {
                        "Only the clan master can upgrade the clan grade."
                    }
                    ClanUpgradeResult::InvalidNpc => {
                        "That NPC cannot upgrade your clan grade."
                    }
                    ClanUpgradeResult::NpcTooFar => "You are too far away from the clan NPC.",
                    ClanUpgradeResult::MaxLevel => "Your clan is already at the maximum grade.",
                    ClanUpgradeResult::InsufficientPoints => {
                        "Your clan does not have enough clan points."
                    }
                };
                message_box_events.send(MessageBoxEvent::Show {
                    message: message.into(),
                    modal: false,
                    ok: None,
                    cancel: None,
                });
            }
            Ok(ServerMessage::ClanMemberList { members }) =>  {
                if let Some(player_entity) = client_entity_list.player_entity {
                    commands.add(move |world: &mut World| {
                        let mut entity_mut = world.entity_mut(player_entity);
                        if let Some(mut clan) = entity_mut.get_mut::<Clan>() {
                            clan.members.clear();

                            for member in members {
                                clan.members.push(ClanMember {
                                    name: member.name,
                                    position: member.position,
                                    contribution: member.contribution,
                                    level: member.level,
                                    job: member.job,
                                    channel_id: member.channel_id,
                                });
                            }
                        }
                    });
                }
            }
            Ok(ServerMessage::ClanInvited { name, clan_unique_id, clan_mark: _, clan_level, clan_name, inviter_entity_id: _ }) => {
                pending_clan_invites.invites.push(crate::resources::PendingClanInvite {
                    inviter_name: name,
                    clan_name,
                    clan_unique_id,
                    clan_level,
                });
            }
            Ok(ServerMessage::ClanInviteResult { response }) => {
                log::info!("Received clan invite result: {:?}", response);
                // TODO: Show invite result message to user
            }
            Ok(ServerMessage::ClanMemberJoined { name }) => {
                log::info!("Clan member joined: {}", name);
                if let Some(world_connection) = world_connection.as_ref() {
                    world_connection
                        .client_message_tx
                        .send(ClientMessage::ClanGetMemberList)
                        .ok();
                }
            }
            Ok(ServerMessage::ClanMemberLeft { name }) => {
                log::info!("Clan member left: {}", name);
                if let Some(player_entity) = client_entity_list.player_entity {
                    commands.add(move |world: &mut World| {
                        let mut entity_mut = world.entity_mut(player_entity);
                        if let Some(mut clan) = entity_mut.get_mut::<Clan>() {
                            clan.members.retain(|member| member.name != name);
                        }

                        clear_visible_character_clan_membership_by_name(world, &name);
                    });
                }
            }
            Ok(ServerMessage::ClanMemberKicked { name }) => {
                log::info!("Clan member kicked: {}", name);
                if let Some(player_entity) = client_entity_list.player_entity {
                    commands.add(move |world: &mut World| {
                        let mut entity_mut = world.entity_mut(player_entity);
                        if let Some(mut clan) = entity_mut.get_mut::<Clan>() {
                            clan.members.retain(|member| member.name != name);
                        }

                        clear_visible_character_clan_membership_by_name(world, &name);
                    });
                }
            }
            Ok(ServerMessage::ClanKicked) => {
                log::info!("You have been kicked from the clan");
                if let Some(player_entity) = client_entity_list.player_entity {
                    commands.add(move |world: &mut World| {
                        let mut entity_mut = world.entity_mut(player_entity);
                        entity_mut.remove::<Clan>();
                        entity_mut.remove::<ClanMembership>();
                    });
                }
            }
            Ok(ServerMessage::ClanDisbanded) => {
                log::info!("Your clan has been disbanded");
                if let Some(player_entity) = client_entity_list.player_entity {
                    commands.add(move |world: &mut World| {
                        let mut entity_mut = world.entity_mut(player_entity);
                        entity_mut.remove::<Clan>();
                        entity_mut.remove::<ClanMembership>();
                    });
                }
            }
            Ok(ServerMessage::CraftInsertGem { update_items }) => {
                if let Some(player_entity) = client_entity_list.player_entity {
                    commands.add(move |world: &mut World| {
                        update_inventory_and_money(world, player_entity, update_items, None);
                    });
                }
            }
            Ok(ServerMessage::CraftInsertGemError { error }) => {
                log::warn!("Gem insertion failed: {:?}", error);
            }
            Ok(ServerMessage::CraftCreateItemSuccess {
                inventory_slot,
                item,
            }) => {
                if let Some(player_entity) = client_entity_list.player_entity {
                    let update_items = vec![(inventory_slot, Some(item))];
                    commands.add(move |world: &mut World| {
                        update_inventory_and_money(world, player_entity, update_items, None);
                    });
                    chatbox_events.send(ChatboxEvent::System("Crafting successful!".to_string()));
                    spawn_effect_events.send(SpawnEffectEvent::OnEntity(
                        player_entity,
                        None,
                        SpawnEffectData::with_path(VfsPathBuf::new(
                            "3DDATA/EFFECT/_SUCCESS_01.EFT",
                        )),
                    ));
                }
            }
            Ok(ServerMessage::CraftCreateItemError { error }) => {
                let craft_failed = matches!(
                    &error,
                    rose_game_common::messages::server::CraftCreateItemError::Failed
                );
                let msg = match error {
                    rose_game_common::messages::server::CraftCreateItemError::Failed => "Crafting failed.",
                    rose_game_common::messages::server::CraftCreateItemError::InvalidCondition => "Crafting failed: invalid condition.",
                    rose_game_common::messages::server::CraftCreateItemError::NeedItem => "Crafting failed: missing materials.",
                    rose_game_common::messages::server::CraftCreateItemError::InvalidItem => "Crafting failed: invalid item.",
                    rose_game_common::messages::server::CraftCreateItemError::NeedSkillLevel => "Crafting failed: insufficient skill level.",
                };
                chatbox_events.send(ChatboxEvent::System(msg.to_string()));
                if craft_failed {
                    if let Some(player_entity) = client_entity_list.player_entity {
                        spawn_effect_events.send(SpawnEffectEvent::OnEntity(
                            player_entity,
                            None,
                            SpawnEffectData::with_path(VfsPathBuf::new(
                                "3DDATA/EFFECT/_FAILED_01.EFT",
                            )),
                        ));
                    }
                }
            }
            Ok(ServerMessage::CraftUpgradeSuccess { update_items }) => {
                if let Some(player_entity) = client_entity_list.player_entity {
                    commands.add(move |world: &mut World| {
                        update_inventory_and_money(world, player_entity, update_items, None);
                    });
                    chatbox_events.send(ChatboxEvent::System("Upgrade successful!".to_string()));
                    spawn_effect_events.send(SpawnEffectEvent::OnEntity(
                        player_entity,
                        None,
                        SpawnEffectData::with_path(VfsPathBuf::new(
                            "3DDATA/EFFECT/_SUCCESS_01.EFT",
                        )),
                    ));
                }
                commands.add(|world: &mut World| {
                    world
                        .resource_mut::<Events<CraftEvent>>()
                        .send(CraftEvent::UpgradeCompleted);
                });
            }
            Ok(ServerMessage::CraftUpgradeFailed { update_items }) => {
                if let Some(player_entity) = client_entity_list.player_entity {
                    commands.add(move |world: &mut World| {
                        update_inventory_and_money(world, player_entity, update_items, None);
                    });
                    chatbox_events.send(ChatboxEvent::System("Upgrade failed!".to_string()));
                    spawn_effect_events.send(SpawnEffectEvent::OnEntity(
                        player_entity,
                        None,
                        SpawnEffectData::with_path(VfsPathBuf::new(
                            "3DDATA/EFFECT/_FAILED_01.EFT",
                        )),
                    ));
                }
                commands.add(|world: &mut World| {
                    world
                        .resource_mut::<Events<CraftEvent>>()
                        .send(CraftEvent::UpgradeCompleted);
                });
            }
            Ok(ServerMessage::CraftDisassembleSuccess { update_items }) => {
                if let Some(player_entity) = client_entity_list.player_entity {
                    commands.add(move |world: &mut World| {
                        update_inventory_and_money(world, player_entity, update_items, None);
                    });
                    chatbox_events.send(ChatboxEvent::System("Disassembly complete!".to_string()));
                }
            }
            Ok(ServerMessage::RepairedItemUsingNpc { .. }) => {
                log::warn!("Received unimplemented ServerMessage::RepairedItemUsingNpc");
            }
            Ok(ServerMessage::LogoutSuccess) => {
                log::warn!("Received unimplemented ServerMessage::LogoutSuccess");
            }
            Ok(ServerMessage::LogoutFailed { .. }) => {
                log::warn!("Received unimplemented ServerMessage::LogoutFailed");
            }
            Ok(ServerMessage::ReturnToCharacterSelect) => {
                log::warn!("Received unimplemented ServerMessage::ReturnToCharacterSelect");
            }
            Ok(ServerMessage::LoginError { .. }) |
            Ok(ServerMessage::LoginSuccess { .. }) |
            Ok(ServerMessage::ChannelList { .. }) |
            Ok(ServerMessage::ChannelListError { .. }) |
            Ok(ServerMessage::JoinServerError {.. }) |
            Ok(ServerMessage::JoinServerSuccess { ..}) |
            Ok(ServerMessage::CharacterList { .. }) |
            Ok(ServerMessage::CharacterListAppend { .. }) |
            Ok(ServerMessage::CreateCharacterSuccess { .. }) |
            Ok(ServerMessage::CreateCharacterError { .. }) |
            Ok(ServerMessage::SelectCharacterSuccess { .. }) |
            Ok(ServerMessage::SelectCharacterError { .. }) |
            Ok(ServerMessage::DeleteCharacterStart { .. }) |
            Ok(ServerMessage::DeleteCharacterCancel { .. }) |
            Ok(ServerMessage::DeleteCharacterError { .. }) => {
                // These should only be login / world server packets, not game server
                log::warn!("Received unexpected game server message");
            }
            Err(crossbeam_channel::TryRecvError::Disconnected) => {
                break Err(ConnectionError::ConnectionLost.into());
            }
            Err(crossbeam_channel::TryRecvError::Empty) => break Ok(()),
        }
    };

    if let Err(error) = result {
        // TODO: Store error somewhere to display to user
        log::warn!("Game server connection error: {}", error);
        social_state.clear();
        commands.remove_resource::<GameConnection>();
    }
}

#[cfg(test)]
mod tests {
    use super::{
        apply_status_effect_updates, clear_missing_status_effect_regen, party_level_up_event,
        reward_money_diff, sync_reward_money,
    };
    use crate::events::ClientEntityEvent;
    use bevy::prelude::Entity;
    use rose_data::{StatusEffectId, StatusEffectType};
    use rose_game_common::{
        components::{
            ActiveStatusEffect, ActiveStatusEffectRegen, Inventory, Money, StatusEffects,
            StatusEffectsRegen,
        },
        messages::server::ActiveStatusEffects,
    };
    use std::time::Duration;

    #[test]
    fn party_level_xp_level_up_emits_party_level_up_event() {
        let player_entity = Entity::from_raw(7);
        assert!(matches!(
            party_level_up_event(player_entity, true),
            Some(ClientEntityEvent::PartyLevelUp(entity)) if entity == player_entity
        ));
    }

    #[test]
    fn party_level_xp_without_level_up_emits_no_event() {
        assert!(matches!(
            party_level_up_event(Entity::from_raw(7), false),
            None
        ));
    }

    #[test]
    fn reward_money_sync_replaces_money_when_total_decreases() {
        let mut inventory = Inventory::default();
        inventory.money = Money(10_000);

        let diff = sync_reward_money(&mut inventory, Money(8_000));

        assert_eq!(diff, -2_000);
        assert_eq!(inventory.money, Money(8_000));
    }

    #[test]
    fn reward_money_sync_replaces_money_when_total_increases() {
        let mut inventory = Inventory::default();
        inventory.money = Money(10_000);

        let diff = sync_reward_money(&mut inventory, Money(12_000));

        assert_eq!(diff, 2_000);
        assert_eq!(inventory.money, Money(12_000));
    }

    #[test]
    fn reward_money_diff_uses_absolute_totals() {
        assert_eq!(reward_money_diff(Money(10_000), Money(8_000)), -2_000);
        assert_eq!(reward_money_diff(Money(10_000), Money(12_000)), 2_000);
    }

    #[test]
    fn update_status_effects_applies_poison_without_fabricating_expire_time() {
        let mut status_effects = StatusEffects::default();
        let mut update_status_effects = ActiveStatusEffects::default();
        let poison = ActiveStatusEffect {
            id: StatusEffectId::new(7).unwrap(),
            value: 9,
        };

        update_status_effects[StatusEffectType::Poisoned] = Some(poison.clone());

        let (updated_hp, updated_mp) =
            apply_status_effect_updates(&mut status_effects, &update_status_effects, &[]);

        let active_poison = status_effects.active[StatusEffectType::Poisoned]
            .as_ref()
            .expect("poison should be applied");
        assert_eq!(active_poison.id, poison.id);
        assert_eq!(active_poison.value, poison.value);
        assert!(status_effects.expire_times[StatusEffectType::Poisoned].is_none());
        assert!(updated_hp.is_none());
        assert!(updated_mp.is_none());
    }

    #[test]
    fn update_status_effects_clear_removes_poison_and_stale_regen() {
        let mut status_effects = StatusEffects::default();
        let mut status_effects_regen = StatusEffectsRegen::default();
        let mut update_status_effects = ActiveStatusEffects::default();

        status_effects.active[StatusEffectType::Poisoned] = Some(ActiveStatusEffect {
            id: StatusEffectId::new(7).unwrap(),
            value: 9,
        });
        status_effects.expire_times[StatusEffectType::Poisoned] = Some(std::time::Instant::now());
        status_effects_regen.regens[StatusEffectType::Poisoned] = Some(ActiveStatusEffectRegen {
            total_value: 30,
            value_per_second: 3,
            applied_value: 6,
            applied_duration: Duration::from_secs(2),
        });
        update_status_effects[StatusEffectType::Poisoned] = None;

        apply_status_effect_updates(&mut status_effects, &update_status_effects, &[]);
        clear_missing_status_effect_regen(&mut status_effects_regen, &update_status_effects);

        assert!(status_effects.active[StatusEffectType::Poisoned].is_none());
        assert!(status_effects.expire_times[StatusEffectType::Poisoned].is_none());
        assert!(status_effects_regen.regens[StatusEffectType::Poisoned].is_none());
    }
}
