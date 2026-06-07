CREATE TABLE user_authorization
(
    access_token                        TEXT PRIMARY KEY NOT NULL,
    twitch_access_token                 TEXT             NOT NULL,
    twitch_refresh_token                TEXT             NOT NULL,
    twitch_authorization_last_validated TIMESTAMPTZ      NOT NULL,
    valid_until                         TIMESTAMPTZ      NOT NULL,
    user_id                             TEXT             NOT NULL,
    user_login                          TEXT             NOT NULL,
    user_name                           TEXT             NOT NULL,
    user_profile_image_url              TEXT             NOT NULL
);
