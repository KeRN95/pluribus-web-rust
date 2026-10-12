-- The phone app's sessions beside the pages' (pluribus-api, /api/v1): one table for both, each
-- session's token kept as its SHA-256 (migration 0007), the pages' sent in a cookie, the app's as
-- a bearer token, so that "Cilësimet" counts the app among the devices signed in and signing out
-- everywhere else ends it too. Each session now says what made it (`client`: the pages, 'web', or
-- the app on an iPhone, 'ios', or an Android phone, 'android'), the phone's name as the app gives
-- it (`device_name`), and when it was last used (`last_used_at`, moved by db::session_user once
-- it is ten minutes old, as users.last_seen_at is), for the devices a list of them would show.
-- Sessions made before this were the pages'.
ALTER TABLE sessions
    ADD COLUMN client varchar(10) NOT NULL DEFAULT 'web' CHECK (client IN ('web', 'ios', 'android')),
    ADD COLUMN device_name varchar(80),
    ADD COLUMN last_used_at timestamptz;
