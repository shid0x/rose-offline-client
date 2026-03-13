use bevy::prelude::{EventReader, EventWriter};
use rose_file_readers::VfsPathBuf;

use crate::events::{ConversationDialogEvent, SystemFuncEvent};

const SYSTEM_FUNC_EVENT_DIALOGS: &[(&str, &str)] = &[
    ("Lunar_Warp_Gate01", "3DDATA/EVENT/OBJECT001.CON"),
    ("mushroom", "3DDATA/EVENT/OBJECT002.CON"),
    ("sandglass", "3DDATA/EVENT/OBJECT003.CON"),
    ("horriblebook", "3DDATA/EVENT/OBJECT004.CON"),
    ("piramid01", "3DDATA/EVENT/OBJECT005.CON"),
    ("piramid03", "3DDATA/EVENT/OBJECT005.CON"),
    ("piramid02", "3DDATA/EVENT/OBJECT006.CON"),
    ("owl", "3DDATA/EVENT/OBJECT007.CON"),
    ("mana", "3DDATA/EVENT/OBJECT008.CON"),
    ("genzistone", "3DDATA/EVENT/OBJECT009.CON"),
];

fn get_event_dialog_path(function_name: &str) -> Option<&'static str> {
    SYSTEM_FUNC_EVENT_DIALOGS
        .iter()
        .find_map(|(mapped_function_name, con_path)| {
            (*mapped_function_name == function_name).then_some(*con_path)
        })
}

pub fn system_func_event_system(
    mut events: EventReader<SystemFuncEvent>,
    mut conversation_dialog_events: EventWriter<ConversationDialogEvent>,
) {
    for event in events.iter() {
        let SystemFuncEvent::CallFunction(function_name, _parameters) = event;

        if let Some(con_path) = get_event_dialog_path(function_name.as_str()) {
            conversation_dialog_events.send(ConversationDialogEvent::OpenEventDialog(
                VfsPathBuf::new(con_path),
            ));
        } else {
            log::warn!("Unimplemented system func function {}", function_name);
        }
    }
}
