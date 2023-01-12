CREATE TABLE message
(
    channel_login  TEXT                     NOT NULL,
    time_received  TIMESTAMP WITH TIME ZONE NOT NULL,
    message_source TEXT                     NOT NULL
);

-- used by the get_messages, purge_messages, run_message_vacuum queries
create index on message(channel_login, time_received);
