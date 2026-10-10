-- The account's own token in the links that stop its e-mail notices without signing in
-- (web/unsubscribe.rs, /ndalo/{token}/...): made, at random, the first time an e-mail goes out,
-- and gone with the account's other contacts when it closes. A link checker that follows one only
-- asks; stopping takes its form, or a mail program's one-click POST (RFC 8058).
ALTER TABLE users ADD COLUMN notice_token varchar(64) UNIQUE;
