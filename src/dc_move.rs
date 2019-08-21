use crate::constants::*;
use crate::context::*;
use crate::filter_mode::{get_filter_mode, FilterMode};
use crate::job::*;
use crate::message::*;
use crate::param::Params;

pub unsafe fn dc_do_heuristics_moves(context: &Context, folder: &str, msg_id: u32) {
    match get_filter_mode(context) {
        FilterMode::Deltachat => {}
        _ => {
            // either filtering is disabled (None) or filtering is performed by the COI server.
            return;
        }
    }

    if !dc_is_inbox(context, folder) && !dc_is_sentbox(context, folder) {
        return;
    }

    if let Ok(msg) = dc_msg_new_load(context, msg_id) {
        if dc_msg_is_setupmessage(&msg) {
            // do not move setup messages;
            // there may be a non-delta device that wants to handle it
            return;
        }

        if dc_is_mvbox(context, folder) {
            dc_update_msg_move_state(context, msg.rfc724_mid, MoveState::Stay);
        }

        // 1 = dc message, 2 = reply to dc message
        if 0 != msg.is_dc_message {
            job_add(
                context,
                Action::MoveMsg,
                msg.id as libc::c_int,
                Params::new(),
                0,
            );
            dc_update_msg_move_state(context, msg.rfc724_mid, MoveState::Moving);
        }
    }
}
