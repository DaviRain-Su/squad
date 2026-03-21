use std::collections::{HashMap, VecDeque};

use crate::protocol::Message;

#[derive(Debug, Default)]
pub struct Mailbox {
    queues: HashMap<String, VecDeque<Message>>,
}

impl Mailbox {
    pub fn push(&mut self, agent_id: impl Into<String>, message: Message) {
        self.queues
            .entry(agent_id.into())
            .or_default()
            .push_back(message);
    }

    pub fn pop(&mut self, agent_id: &str) -> Option<Message> {
        let queue = self.queues.get_mut(agent_id)?;
        let message = queue.pop_front();
        if queue.is_empty() {
            self.queues.remove(agent_id);
        }
        message
    }
}

#[cfg(test)]
mod tests {
    use crate::protocol::Message;

    use super::Mailbox;

    #[test]
    fn pops_messages_in_fifo_order() {
        let mut mailbox = Mailbox::default();
        mailbox.push("cc", Message::new("assistant", "first"));
        mailbox.push("cc", Message::new("assistant", "second"));

        let first = mailbox.pop("cc").expect("first message");
        let second = mailbox.pop("cc").expect("second message");

        assert_eq!(first.content, "first");
        assert_eq!(second.content, "second");
        assert!(mailbox.pop("cc").is_none());
    }
}
