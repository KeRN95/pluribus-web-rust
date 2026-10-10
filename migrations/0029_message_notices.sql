-- Notices of unread messages (src/notices.rs): every minute the server looks for messages someone
-- hasn't read a few minutes after they came, among the conversations written in over the last day,
-- found by when their last message came. Each notice it sends is a row in notifications
-- (NEW_MESSAGE, with the conversation in its payload), which says when the last one went out.
CREATE INDEX idx_conversations_last_message ON conversations (last_message_at) WHERE last_message_at IS NOT NULL;
