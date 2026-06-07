CREATE TABLE channel
(
    channel_login TEXT PRIMARY KEY         NOT NULL,
    ignored_at    TIMESTAMP WITH TIME ZONE          DEFAULT NULL,
    last_access   TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT now()
);
