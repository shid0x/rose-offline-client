use bevy::math::{Vec2, Vec3Swizzles};
use chrono::{Datelike, Timelike};
use std::{
    collections::HashSet,
    num::NonZeroU8,
    ops::RangeInclusive,
    sync::{Mutex, OnceLock},
};

use rose_data::QuestTrigger;
use rose_file_readers::{
    QsdAbilityType, QsdClanLevel, QsdClanPoints, QsdClanPosition, QsdCondition,
    QsdConditionOperator, QsdEquipmentIndex, QsdItem, QsdServerChannelId, QsdSkillId,
    QsdTeamNumber, QsdVariableType, QsdZoneId,
};

use crate::{
    bundles::ability_values_get_value,
    components::PartyOwner,
    scripting::{
        quest::get_quest_variable, QuestFunctionContext, ScriptFunctionContext,
        ScriptFunctionResources,
    },
};

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum ConditionEvaluation {
    Passed,
    Failed,
    UnsupportedPassed,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum QuestConditionCheckResult {
    Passed,
    FailedCondition,
    UnsupportedCondition,
}

fn quest_condition_operator<T: PartialEq + PartialOrd>(
    operator: QsdConditionOperator,
    value_lhs: T,
    value_rhs: T,
) -> bool {
    match operator {
        QsdConditionOperator::Equals => value_lhs == value_rhs,
        QsdConditionOperator::GreaterThan => value_lhs > value_rhs,
        QsdConditionOperator::GreaterThanEqual => value_lhs >= value_rhs,
        QsdConditionOperator::LessThan => value_lhs < value_rhs,
        QsdConditionOperator::LessThanEqual => value_lhs <= value_rhs,
        QsdConditionOperator::NotEqual => value_lhs != value_rhs,
    }
}

fn quest_condition_ability_value(
    script_resources: &ScriptFunctionResources,
    script_context: &mut ScriptFunctionContext,
    _quest_context: &mut QuestFunctionContext,
    ability_type: QsdAbilityType,
    operator: QsdConditionOperator,
    compare_value: i32,
) -> bool {
    let character = script_context.query_player.single();

    let ability_type = script_resources
        .game_data
        .data_decoder
        .decode_ability_type(ability_type.get());
    if ability_type.is_none() {
        return false;
    }

    let current_value = ability_values_get_value(
        ability_type.unwrap(),
        character.ability_values,
        Some(character.character_info),
        Some(character.experience_points),
        Some(character.health_points),
        Some(character.inventory),
        Some(character.level),
        Some(character.mana_points),
        Some(character.move_speed),
        Some(character.skill_points),
        Some(character.stamina),
        Some(character.stat_points),
        Some(character.team),
        Some(character.union_membership),
    )
    .unwrap_or(0);

    quest_condition_operator(operator, current_value, compare_value)
}

fn quest_condition_check_switch(
    _script_resources: &ScriptFunctionResources,
    script_context: &mut ScriptFunctionContext,
    _quest_context: &mut QuestFunctionContext,
    switch_id: usize,
    value: bool,
) -> bool {
    let quest_state = script_context.query_quest.single();

    if let Some(switch_value) = quest_state.quest_switches.get(switch_id) {
        return *switch_value == value;
    }

    false
}

fn quest_condition_quest_item(
    script_resources: &ScriptFunctionResources,
    script_context: &mut ScriptFunctionContext,
    quest_context: &mut QuestFunctionContext,
    item: Option<QsdItem>,
    equipment_index: Option<QsdEquipmentIndex>,
    required_count: u32,
    operator: QsdConditionOperator,
) -> bool {
    let item_reference = item.and_then(|item| {
        script_resources
            .game_data
            .data_decoder
            .decode_item_reference(item.item_number, item.item_type)
    });

    let equipment_index = equipment_index.and_then(|equipment_index| {
        script_resources
            .game_data
            .data_decoder
            .decode_equipment_index(equipment_index.get())
    });

    let quest_state = script_context.query_quest.single();
    let character = script_context.query_player.single();

    if let Some(equipment_index) = equipment_index {
        item_reference
            == character
                .equipment
                .get_equipment_item(equipment_index)
                .map(|item| item.item)
    } else {
        let quantity = if let Some(item_reference) = item_reference {
            if item_reference.item_type.is_quest_item() {
                // Check selected quest item
                if let Some(selected_quest_index) = quest_context.selected_quest_index {
                    quest_state
                        .get_quest(selected_quest_index)
                        .and_then(|active_quest| active_quest.find_item(item_reference))
                        .map(|quest_item| quest_item.get_quantity())
                        .unwrap_or(0)
                } else {
                    0
                }
            } else {
                // Check inventory
                character
                    .inventory
                    .find_item(item_reference)
                    .and_then(|slot| character.inventory.get_item(slot))
                    .map(|inventory_item| inventory_item.get_quantity())
                    .unwrap_or(0)
            }
        } else {
            0
        };

        quest_condition_operator(operator, quantity, required_count)
    }
}

fn quest_condition_quest_variable(
    script_resources: &ScriptFunctionResources,
    script_context: &mut ScriptFunctionContext,
    quest_context: &mut QuestFunctionContext,
    variable_type: QsdVariableType,
    variable_id: usize,
    operator: QsdConditionOperator,
    value: i32,
) -> bool {
    if let Some(variable_value) = get_quest_variable(
        script_resources,
        script_context,
        quest_context,
        variable_type,
        variable_id,
    ) {
        quest_condition_operator(operator, variable_value, value)
    } else {
        false
    }
}

fn quest_condition_select_quest(
    _script_resources: &ScriptFunctionResources,
    script_context: &mut ScriptFunctionContext,
    quest_context: &mut QuestFunctionContext,
    quest_id: usize,
) -> bool {
    let quest_state = script_context.query_quest.single();

    if let Some(quest_index) = quest_state.find_active_quest_index(quest_id) {
        quest_context.selected_quest_index = Some(quest_index);
        return true;
    }

    false
}

fn quest_condition_clan_position(
    script_resources: &ScriptFunctionResources,
    script_context: &mut ScriptFunctionContext,
    _quest_context: &mut QuestFunctionContext,
    operator: QsdConditionOperator,
    compare_value: QsdClanPosition,
) -> bool {
    let character = script_context.query_player.single();
    let value = character
        .clan_membership
        .and_then(|clan_membership| {
            script_resources
                .game_data
                .data_decoder
                .encode_clan_member_position(clan_membership.position)
        })
        .unwrap_or(0);
    quest_condition_operator(operator, value, compare_value)
}

fn quest_condition_in_clan(
    _script_resources: &ScriptFunctionResources,
    script_context: &mut ScriptFunctionContext,
    _quest_context: &mut QuestFunctionContext,
    in_clan: bool,
) -> bool {
    let character = script_context.query_player.single();
    character.clan_membership.is_some() == in_clan
}

fn quest_condition_position(
    script_resources: &ScriptFunctionResources,
    script_context: &mut ScriptFunctionContext,
    zone_id: QsdZoneId,
    position: Vec2,
    distance: i32,
) -> bool {
    if script_resources
        .current_zone
        .as_ref()
        .map_or(true, |current_zone| {
            current_zone.id.get() as usize != zone_id
        })
    {
        return false;
    }

    script_context
        .query_player
        .single()
        .position
        .position
        .xy()
        .distance_squared(position)
        < (distance as f32 * distance as f32)
}

fn quest_condition_world_time(
    script_resources: &ScriptFunctionResources,
    range: &RangeInclusive<u32>,
) -> bool {
    range.contains(&script_resources.world_time.ticks.get_world_time())
}

fn quest_condition_month_day_time(
    month_day: Option<NonZeroU8>,
    day_minutes_range: &RangeInclusive<i32>,
) -> bool {
    let local_time = chrono::Local::now();

    if let Some(month_day) = month_day {
        if month_day.get() as u32 != local_time.day() {
            return false;
        }
    }

    let local_day_minutes = local_time.hour() as i32 + local_time.minute() as i32;
    day_minutes_range.contains(&local_day_minutes)
}

fn quest_condition_week_day_time(week_day: u8, day_minutes_range: &RangeInclusive<i32>) -> bool {
    let local_time = chrono::Local::now();

    if week_day as u32 != local_time.weekday().num_days_from_sunday() {
        return false;
    }

    let local_day_minutes = local_time.hour() as i32 + local_time.minute() as i32;
    day_minutes_range.contains(&local_day_minutes)
}

fn quest_condition_have_skill(
    script_context: &mut ScriptFunctionContext,
    skill_id_range: &RangeInclusive<QsdSkillId>,
    have: bool,
) -> bool {
    let character = script_context.query_player.single();
    for page in character.skill_list.pages.iter() {
        for skill_id in page.skills.iter().filter_map(|x| *x) {
            if skill_id_range.contains(&(skill_id.get() as QsdSkillId)) {
                return have;
            }
        }
    }

    !have
}

fn quest_condition_team_number(
    script_context: &mut ScriptFunctionContext,
    range: &RangeInclusive<QsdTeamNumber>,
) -> bool {
    range.contains(&(script_context.query_player.single().team.id as QsdTeamNumber))
}

fn quest_condition_server_channel_number(
    channel_range: &RangeInclusive<QsdServerChannelId>,
) -> bool {
    // TODO: Confirm channel support and replace this with the actual current channel id.
    channel_range.contains(&1)
}

fn quest_condition_party(
    script_context: &mut ScriptFunctionContext,
    is_leader: bool,
    level_operator: QsdConditionOperator,
    level: i32,
) -> bool {
    let character = script_context.query_player.single();
    if let Some(party_info) = character.party_info {
        if is_leader && !matches!(party_info.owner, PartyOwner::Player) {
            return false;
        }

        return quest_condition_operator(level_operator, party_info.level, level);
    }

    false
}

fn quest_condition_party_member_count(
    script_context: &mut ScriptFunctionContext,
    range: &RangeInclusive<usize>,
) -> bool {
    let character = script_context.query_player.single();
    character.party_info.map_or(false, |party_info| {
        range.contains(&party_info.members.len())
    })
}

fn quest_condition_clan_contribution(
    script_context: &mut ScriptFunctionContext,
    operator: QsdConditionOperator,
    compare_value: QsdClanPoints,
) -> bool {
    let character = script_context.query_player.single();
    let value = character
        .clan_membership
        .map_or(0, |clan_membership| clan_membership.contribution.0 as i64);
    quest_condition_operator(operator, value, compare_value as i64)
}

fn quest_condition_clan_level(
    script_context: &mut ScriptFunctionContext,
    operator: QsdConditionOperator,
    compare_value: QsdClanLevel,
) -> bool {
    let character = script_context.query_player.single();
    let value = character
        .clan
        .map_or(0, |clan| clan.level.get() as QsdClanLevel);
    quest_condition_operator(operator, value, compare_value)
}

fn quest_condition_clan_points(
    script_context: &mut ScriptFunctionContext,
    operator: QsdConditionOperator,
    compare_value: QsdClanPoints,
) -> bool {
    let character = script_context.query_player.single();
    let value = character.clan.map_or(0, |clan| clan.points.0 as i64);
    quest_condition_operator(operator, value, compare_value as i64)
}

fn quest_condition_clan_money(
    script_context: &mut ScriptFunctionContext,
    operator: QsdConditionOperator,
    compare_value: i32,
) -> bool {
    let character = script_context.query_player.single();
    let value = character.clan.map_or(0, |clan| clan.money.0);
    quest_condition_operator(operator, value, compare_value as i64)
}

fn quest_condition_clan_member_count(
    script_context: &mut ScriptFunctionContext,
    operator: QsdConditionOperator,
    compare_value: usize,
) -> bool {
    let character = script_context.query_player.single();
    let value = character.clan.map_or(0, |clan| clan.members.len());
    quest_condition_operator(operator, value, compare_value)
}

fn quest_condition_have_clan_skill(
    script_context: &mut ScriptFunctionContext,
    skill_id_range: &RangeInclusive<QsdSkillId>,
    have: bool,
) -> bool {
    let character = script_context.query_player.single();
    if let Some(clan) = character.clan {
        for skill_id in clan.skills.iter() {
            if skill_id_range.contains(&(skill_id.get() as QsdSkillId)) {
                return have;
            }
        }
    }

    !have
}

fn log_unsupported_condition_once(trigger_name: &str, condition: &QsdCondition) {
    static LOGGED_UNSUPPORTED: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

    let key = format!("{}::{:?}", trigger_name, condition);
    if let Ok(mut seen) = LOGGED_UNSUPPORTED
        .get_or_init(|| Mutex::new(HashSet::new()))
        .lock()
    {
        if seen.insert(key) {
            log::warn!(
                target: "quest",
                "Unsupported condition in trigger {}: {:?}. Assuming true for client precheck.",
                trigger_name,
                condition
            );
        }
    } else {
        log::warn!(
            target: "quest",
            "Unsupported condition in trigger {}: {:?}. Assuming true for client precheck.",
            trigger_name,
            condition
        );
    }
}

pub fn quest_trigger_check_conditions(
    script_resources: &ScriptFunctionResources,
    script_context: &mut ScriptFunctionContext,
    quest_context: &mut QuestFunctionContext,
    quest_trigger: &QuestTrigger,
) -> QuestConditionCheckResult {
    let mut had_unsupported_conditions = false;

    for condition in quest_trigger.conditions.iter() {
        let result = match *condition {
            QsdCondition::AbilityValue {
                ability_type,
                operator,
                value,
            } => quest_condition_ability_value(
                script_resources,
                script_context,
                quest_context,
                ability_type,
                operator,
                value,
            )
            .then_some(ConditionEvaluation::Passed)
            .unwrap_or(ConditionEvaluation::Failed),
            QsdCondition::QuestItem {
                item,
                equipment_index,
                required_count,
                operator,
            } => quest_condition_quest_item(
                script_resources,
                script_context,
                quest_context,
                item,
                equipment_index,
                required_count,
                operator,
            )
            .then_some(ConditionEvaluation::Passed)
            .unwrap_or(ConditionEvaluation::Failed),
            QsdCondition::QuestVariable {
                variable_type,
                variable_id,
                operator,
                value,
            } => quest_condition_quest_variable(
                script_resources,
                script_context,
                quest_context,
                variable_type,
                variable_id,
                operator,
                value,
            )
            .then_some(ConditionEvaluation::Passed)
            .unwrap_or(ConditionEvaluation::Failed),
            QsdCondition::QuestSwitch { id, value } => quest_condition_check_switch(
                script_resources,
                script_context,
                quest_context,
                id,
                value,
            )
            .then_some(ConditionEvaluation::Passed)
            .unwrap_or(ConditionEvaluation::Failed),
            QsdCondition::SelectQuest { id } => {
                quest_condition_select_quest(script_resources, script_context, quest_context, id)
                    .then_some(ConditionEvaluation::Passed)
                    .unwrap_or(ConditionEvaluation::Failed)
            }
            QsdCondition::Position {
                zone,
                x,
                y,
                distance,
            } => quest_condition_position(
                script_resources,
                script_context,
                zone,
                Vec2::new(x, y),
                distance,
            )
            .then_some(ConditionEvaluation::Passed)
            .unwrap_or(ConditionEvaluation::Failed),
            QsdCondition::WorldTime { ref range } => {
                quest_condition_world_time(script_resources, range)
                    .then_some(ConditionEvaluation::Passed)
                    .unwrap_or(ConditionEvaluation::Failed)
            }
            QsdCondition::MonthDayTime {
                month_day,
                ref day_minutes_range,
            } => quest_condition_month_day_time(month_day, day_minutes_range)
                .then_some(ConditionEvaluation::Passed)
                .unwrap_or(ConditionEvaluation::Failed),
            QsdCondition::WeekDayTime {
                week_day,
                ref day_minutes_range,
            } => quest_condition_week_day_time(week_day, day_minutes_range)
                .then_some(ConditionEvaluation::Passed)
                .unwrap_or(ConditionEvaluation::Failed),
            QsdCondition::HasSkill { id, has_skill } => {
                quest_condition_have_skill(script_context, &(id..=id), has_skill)
                    .then_some(ConditionEvaluation::Passed)
                    .unwrap_or(ConditionEvaluation::Failed)
            }
            QsdCondition::HasSkillInRange {
                ref range,
                has_skill,
            } => quest_condition_have_skill(script_context, range, has_skill)
                .then_some(ConditionEvaluation::Passed)
                .unwrap_or(ConditionEvaluation::Failed),
            QsdCondition::TeamNumber { ref range } => {
                quest_condition_team_number(script_context, range)
                    .then_some(ConditionEvaluation::Passed)
                    .unwrap_or(ConditionEvaluation::Failed)
            }
            QsdCondition::ServerChannelNumber { ref range } => {
                quest_condition_server_channel_number(range)
                    .then_some(ConditionEvaluation::Passed)
                    .unwrap_or(ConditionEvaluation::Failed)
            }
            QsdCondition::Party {
                is_leader,
                level_operator,
                level,
            } => quest_condition_party(script_context, is_leader, level_operator, level)
                .then_some(ConditionEvaluation::Passed)
                .unwrap_or(ConditionEvaluation::Failed),
            QsdCondition::PartyMemberCount { ref range } => {
                quest_condition_party_member_count(script_context, range)
                    .then_some(ConditionEvaluation::Passed)
                    .unwrap_or(ConditionEvaluation::Failed)
            }
            QsdCondition::ClanPosition { operator, value } => quest_condition_clan_position(
                script_resources,
                script_context,
                quest_context,
                operator,
                value,
            )
            .then_some(ConditionEvaluation::Passed)
            .unwrap_or(ConditionEvaluation::Failed),
            QsdCondition::ClanPointContribution { operator, value } => {
                quest_condition_clan_contribution(script_context, operator, value)
                    .then_some(ConditionEvaluation::Passed)
                    .unwrap_or(ConditionEvaluation::Failed)
            }
            QsdCondition::ClanLevel { operator, value } => {
                quest_condition_clan_level(script_context, operator, value)
                    .then_some(ConditionEvaluation::Passed)
                    .unwrap_or(ConditionEvaluation::Failed)
            }
            QsdCondition::ClanPoints { operator, value } => {
                quest_condition_clan_points(script_context, operator, value)
                    .then_some(ConditionEvaluation::Passed)
                    .unwrap_or(ConditionEvaluation::Failed)
            }
            QsdCondition::ClanMoney { operator, value } => {
                quest_condition_clan_money(script_context, operator, value)
                    .then_some(ConditionEvaluation::Passed)
                    .unwrap_or(ConditionEvaluation::Failed)
            }
            QsdCondition::ClanMemberCount { operator, value } => {
                quest_condition_clan_member_count(script_context, operator, value)
                    .then_some(ConditionEvaluation::Passed)
                    .unwrap_or(ConditionEvaluation::Failed)
            }
            QsdCondition::HasClanSkill { id, has_skill } => {
                quest_condition_have_clan_skill(script_context, &(id..=id), has_skill)
                    .then_some(ConditionEvaluation::Passed)
                    .unwrap_or(ConditionEvaluation::Failed)
            }
            QsdCondition::HasClanSkillInRange {
                ref range,
                has_skill,
            } => quest_condition_have_clan_skill(script_context, range, has_skill)
                .then_some(ConditionEvaluation::Passed)
                .unwrap_or(ConditionEvaluation::Failed),
            QsdCondition::HasClan { has_clan } => {
                quest_condition_in_clan(script_resources, script_context, quest_context, has_clan)
                    .then_some(ConditionEvaluation::Passed)
                    .unwrap_or(ConditionEvaluation::Failed)
            }
            // Conditions that require server-only state on this client implementation.
            QsdCondition::RandomPercent { .. }
            | QsdCondition::ObjectVariable { .. }
            | QsdCondition::ObjectZoneTime { .. }
            | QsdCondition::ObjectDistance { .. }
            | QsdCondition::CompareNpcVariables { .. }
            | QsdCondition::SelectEventObject { .. }
            | QsdCondition::SelectNpc { .. } => {
                log_unsupported_condition_once(&quest_trigger.name, condition);
                ConditionEvaluation::UnsupportedPassed
            }
        };

        match result {
            ConditionEvaluation::Failed => {
                log::debug!(
                    target: "quest",
                    "Condition failed for trigger {}: {:?}",
                    quest_trigger.name,
                    condition
                );
                return QuestConditionCheckResult::FailedCondition;
            }
            ConditionEvaluation::Passed => {
                log::debug!(
                    target: "quest",
                    "Condition succeeded for trigger {}: {:?}",
                    quest_trigger.name,
                    condition
                );
            }
            ConditionEvaluation::UnsupportedPassed => {
                had_unsupported_conditions = true;
            }
        }
    }

    if had_unsupported_conditions {
        QuestConditionCheckResult::UnsupportedCondition
    } else {
        QuestConditionCheckResult::Passed
    }
}

#[cfg(test)]
mod tests {
    use super::quest_condition_operator;
    use rose_file_readers::QsdConditionOperator;

    #[test]
    fn test_quest_condition_operator_variants() {
        assert!(quest_condition_operator(QsdConditionOperator::Equals, 5, 5));
        assert!(quest_condition_operator(
            QsdConditionOperator::GreaterThan,
            6,
            5
        ));
        assert!(quest_condition_operator(
            QsdConditionOperator::GreaterThanEqual,
            5,
            5
        ));
        assert!(quest_condition_operator(
            QsdConditionOperator::LessThan,
            4,
            5
        ));
        assert!(quest_condition_operator(
            QsdConditionOperator::LessThanEqual,
            5,
            5
        ));
        assert!(quest_condition_operator(
            QsdConditionOperator::NotEqual,
            4,
            5
        ));
    }
}
