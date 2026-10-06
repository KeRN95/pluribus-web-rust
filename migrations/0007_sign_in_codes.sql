-- Signing in without a password: the visitor gives a phone number or an e-mail address, gets a
-- six-digit code there, and typing it back proves the contact is theirs. Codes and session tokens
-- are stored as SHA-256 hashes, so a copy of the database can't be used to sign in.

CREATE TABLE login_codes (
    id          uuid PRIMARY KEY DEFAULT uuid_v7(),
    -- +355691234567 or an e-mail address in lower case.
    contact     varchar(254) NOT NULL,
    code_hash   bytea NOT NULL,
    -- Wrong guesses; the code stops working after five.
    attempts    smallint NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    -- Where the request came from, to limit how many codes one network can ask for.
    client_ip   inet,
    created_at  timestamptz NOT NULL DEFAULT now(),
    expires_at  timestamptz NOT NULL,
    used_at     timestamptz
);

-- The rate limits count recent codes per contact and per network.
CREATE INDEX idx_login_codes_contact ON login_codes (contact, created_at DESC);
CREATE INDEX idx_login_codes_ip ON login_codes (client_ip, created_at DESC);

CREATE TABLE sessions (
    token_hash  bytea PRIMARY KEY,
    user_id     uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at  timestamptz NOT NULL DEFAULT now(),
    expires_at  timestamptz NOT NULL
);

CREATE INDEX idx_sessions_user ON sessions (user_id);
