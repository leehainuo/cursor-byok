//! Assigns missing user input identities after Bidi append ordering and deduplication.

use crate::cursor::protocol::proto::agent::v1 as pb;

pub(super) fn assign_message_id(
    message: &mut pb::AgentClientMessage,
    request_id: &str,
    seqno: i64,
) {
    use pb::agent_client_message::Message;

    let action = match message.message.as_mut() {
        Some(Message::RunRequest(request)) => request.action.as_mut(),
        Some(Message::ConversationAction(action)) => Some(action),
        _ => None,
    };
    let Some(pb::conversation_action::Action::UserMessageAction(action)) =
        action.and_then(|action| action.action.as_mut())
    else {
        return;
    };
    let Some(user) = action.user_message.as_mut() else {
        return;
    };
    if user.message_id.is_empty() {
        // One transport can carry several turns. The append sequence identifies
        // each input while retransmissions retain the same identity.
        user.message_id = format!("byok-user:{request_id}:{seqno}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(message_id: &str) -> pb::AgentClientMessage {
        pb::AgentClientMessage {
            message: Some(pb::agent_client_message::Message::ConversationAction(
                pb::ConversationAction {
                    action: Some(pb::conversation_action::Action::UserMessageAction(
                        pb::UserMessageAction {
                            user_message: Some(pb::UserMessage {
                                message_id: message_id.into(),
                                ..Default::default()
                            }),
                            ..Default::default()
                        },
                    )),
                    ..Default::default()
                },
            )),
        }
    }

    #[test]
    fn retries_keep_the_same_identity_and_new_inputs_get_distinct_identities() {
        let mut first = message("");
        let mut retry = first.clone();
        let mut next = first.clone();
        assign_message_id(&mut first, "request", 1);
        assign_message_id(&mut retry, "request", 1);
        assign_message_id(&mut next, "request", 2);
        assert_eq!(first, retry);
        assert_ne!(first, next);
    }

    #[test]
    fn client_supplied_identities_are_preserved() {
        let mut original = message("client-id");
        let expected = original.clone();
        assign_message_id(&mut original, "request", 1);
        assert_eq!(original, expected);
    }
}
