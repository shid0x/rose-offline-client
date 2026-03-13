use bevy::prelude::EventReader;
use rose_game_common::messages::client::ClientMessage;

use crate::{
    events::QuestTriggerEvent,
    scripting::{
        quest_apply_rewards, quest_check_conditions, quest_check_result_allows_trigger,
        ScriptFunctionContext, ScriptFunctionResources,
    },
};

pub fn quest_trigger_system(
    mut quest_trigger_events: EventReader<QuestTriggerEvent>,
    mut script_context: ScriptFunctionContext,
    script_resources: ScriptFunctionResources,
) {
    for event in quest_trigger_events.iter() {
        match *event {
            QuestTriggerEvent::ApplyRewards(trigger_hash) => {
                quest_apply_rewards(&script_resources, &mut script_context, trigger_hash).ok();
            }
            QuestTriggerEvent::DoTrigger(trigger_hash) => {
                let check_result =
                    quest_check_conditions(&script_resources, &mut script_context, trigger_hash);
                if quest_check_result_allows_trigger(check_result) {
                    if let Some(game_connection) = script_resources.game_connection.as_ref() {
                        game_connection
                            .client_message_tx
                            .send(ClientMessage::QuestTrigger {
                                trigger: trigger_hash,
                            })
                            .ok();
                    }
                } else {
                    log::debug!(
                        target: "quest",
                        "QuestTriggerEvent::DoTrigger blocked by precheck: {:?}",
                        check_result
                    );
                }
            }
        }
    }
}
