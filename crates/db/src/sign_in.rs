//! Signing in with a code sent to a phone or an e-mail address, sessions and users.

use super::*;

/// How many sign-in codes a contact and a network (client IP) asked for in the last hour, and how
/// many seconds ago the contact's latest one went out.
pub struct CodeRequests {
    pub for_contact: i64,
    pub from_ip: i64,
    pub seconds_since_last: Option<f64>,
}

pub async fn code_requests(db: &PgPool, contact: &str, ip: Option<&str>) -> sqlx::Result<CodeRequests> {
    sqlx::query_as!(
        CodeRequests,
        r#"SELECT
               (SELECT count(*) FROM login_codes
                WHERE contact = $1 AND created_at > now() - interval '1 hour') AS "for_contact!",
               (SELECT count(*) FROM login_codes
                WHERE client_ip = $2::text::inet AND created_at > now() - interval '1 hour') AS "from_ip!",
               (SELECT extract(epoch FROM now() - max(created_at))::float8 FROM login_codes
                WHERE contact = $1) AS seconds_since_last"#,
        contact,
        ip,
    )
    .fetch_one(db)
    .await
}

/// What a sign-in code carries besides the contact: a listing to publish once it is confirmed
/// (posted before signing in), a name for a new account, or the account the contact joins
/// ("Cilësimet", web/settings.rs), which then signs no one in.
#[derive(Debug, Default, Clone)]
pub struct CodePurpose {
    pub listing_id: Option<Uuid>,
    pub display_name: Option<String>,
    pub adds_to: Option<Uuid>,
}

pub async fn create_login_code(
    db: &PgPool,
    contact: &str,
    code_hash: &[u8],
    ip: Option<&str>,
    minutes: i32,
    purpose: &CodePurpose,
) -> sqlx::Result<Uuid> {
    sqlx::query_scalar!(
        "INSERT INTO login_codes (contact, code_hash, client_ip, expires_at, listing_id, display_name, user_id)
         VALUES ($1, $2, $3::text::inet, now() + make_interval(mins => $4), $5, $6, $7)
         RETURNING id",
        contact,
        code_hash,
        ip,
        minutes,
        purpose.listing_id,
        purpose.display_name,
        purpose.adds_to,
    )
    .fetch_one(db)
    .await
}

/// Forgets the sign-in codes asked for more than a day ago, used or not, with the contact and the
/// network (client IP) they were asked from, as the privacy page says: the limits look back an hour
/// (code_requests) and a code lasts minutes, so nothing needs them after that. How many went.
pub async fn forget_old_codes(db: &PgPool) -> sqlx::Result<u64> {
    let done = sqlx::query!("DELETE FROM login_codes WHERE created_at < now() - interval '1 day'").execute(db).await?;
    Ok(done.rows_affected())
}

/// A code that never went out (the provider refused it): gone, so it neither waits nor counts.
pub async fn delete_login_code(db: &PgPool, id: Uuid) -> sqlx::Result<()> {
    sqlx::query!("DELETE FROM login_codes WHERE id = $1", id).execute(db).await.map(|_| ())
}

/// A code waiting to be typed in: who it went to, whether it publishes a listing, and whether it
/// adds its contact to an account, if it can still be used.
pub struct PendingCode {
    pub contact: String,
    pub publishes: bool,
    /// The listing it publishes waits for a moderator meanwhile (review.rs, NEW_ACCOUNT).
    pub reviewed: bool,
    pub adds: bool,
}

pub async fn pending_login_code(db: &PgPool, id: Uuid, max_attempts: i16) -> sqlx::Result<Option<PendingCode>> {
    sqlx::query_as!(
        PendingCode,
        r#"SELECT contact, listing_id IS NOT NULL AS "publishes!",
                  EXISTS (SELECT 1 FROM listings l WHERE l.id = login_codes.listing_id AND l.status = 'PENDING_REVIEW') AS "reviewed!",
                  user_id IS NOT NULL AS "adds!" FROM login_codes
           WHERE id = $1 AND used_at IS NULL AND expires_at > now() AND attempts < $2"#,
        id,
        max_attempts,
    )
    .fetch_optional(db)
    .await
}

pub enum CodeCheck {
    /// Right: the code is used up, and the contact it went to is proven.
    Signed { contact: String, purpose: CodePurpose },
    /// Wrong, with this many tries left.
    Wrong(i16),
    /// Expired, used, or tried too often: a new code is needed.
    Gone,
}

/// Checks a typed code. The row is locked, so two guesses at once can't both count as the last.
pub async fn check_login_code(db: &PgPool, id: Uuid, code_hash: &[u8], max_attempts: i16) -> sqlx::Result<CodeCheck> {
    let mut tx = db.begin().await?;
    let Some(row) = sqlx::query!(
        "SELECT contact, code_hash, attempts, listing_id, display_name, user_id FROM login_codes
         WHERE id = $1 AND used_at IS NULL AND expires_at > now()
         FOR UPDATE",
        id,
    )
    .fetch_optional(&mut *tx)
    .await?
    else {
        return Ok(CodeCheck::Gone);
    };
    if row.attempts >= max_attempts {
        return Ok(CodeCheck::Gone);
    }
    let check = if row.code_hash == code_hash {
        sqlx::query!("UPDATE login_codes SET used_at = now() WHERE id = $1", id).execute(&mut *tx).await?;
        CodeCheck::Signed {
            contact: row.contact,
            purpose: CodePurpose { listing_id: row.listing_id, display_name: row.display_name, adds_to: row.user_id },
        }
    } else {
        sqlx::query!("UPDATE login_codes SET attempts = attempts + 1 WHERE id = $1", id).execute(&mut *tx).await?;
        match max_attempts - row.attempts - 1 {
            0 => CodeCheck::Gone,
            left => CodeCheck::Wrong(left),
        }
    };
    tx.commit().await?;
    Ok(check)
}

/// The account that owns a phone number or e-mail address.
pub struct Account {
    pub id: Uuid,
    /// PENDING (created, contact not yet confirmed), ACTIVE or SUSPENDED.
    pub status: String,
}

/// The account for this phone number or e-mail address, created if there is none: PENDING, with
/// this name, until its contact is confirmed (`confirm_account`).
pub async fn account_for_contact(
    db: &PgPool,
    phone: Option<&str>,
    email: Option<&str>,
    name: Option<&str>,
) -> sqlx::Result<Account> {
    if let Some(account) = account_by_contact(db, phone, email).await? {
        return Ok(account);
    }
    let subject = match (phone, email) {
        (Some(phone), _) => format!("phone:{phone}"),
        (None, Some(email)) => format!("email:{email}"),
        (None, None) => unreachable!("an account always has a phone number or an e-mail address"),
    };
    // Two first sign-ins at once: one insert wins, and both find its row.
    sqlx::query!(
        "INSERT INTO users (auth_subject, phone_e164, email, display_name, status)
         VALUES ($1, $2::text::e164_phone, $3::text::citext, coalesce($4, 'Përdorues'), 'PENDING')
         ON CONFLICT DO NOTHING",
        subject,
        phone,
        email,
        name,
    )
    .execute(db)
    .await?;
    account_by_contact(db, phone, email).await?.ok_or(sqlx::Error::RowNotFound)
}

/// The contact is confirmed: a new account becomes active, and takes the name given when it was
/// created if it has none yet.
pub async fn confirm_account(db: &PgPool, id: Uuid, name: Option<&str>) -> sqlx::Result<()> {
    sqlx::query!(
        "UPDATE users
         SET status = CASE WHEN status = 'PENDING' THEN 'ACTIVE' ELSE status END,
             display_name = CASE WHEN display_name = 'Përdorues' THEN coalesce($2, display_name) ELSE display_name END
         WHERE id = $1",
        id,
        name,
    )
    .execute(db)
    .await
    .map(|_| ())
}

/// The account this phone number or e-mail address has, if any.
pub async fn account_by_contact(db: &PgPool, phone: Option<&str>, email: Option<&str>) -> sqlx::Result<Option<Account>> {
    sqlx::query_as!(
        Account,
        "SELECT id, status FROM users WHERE (phone_e164 = $1 OR email = $2::text::citext) AND status <> 'DELETED'",
        phone,
        email,
    )
    .fetch_optional(db)
    .await
}

/// The account a listing posted without signing in goes under: the one its phone number or e-mail
/// address has, or a new one, active at once (posting needs no code), under the name typed or else
/// "Përdorues".
pub async fn poster_account(db: &PgPool, phone: Option<&str>, email: Option<&str>, name: Option<&str>) -> sqlx::Result<Account> {
    let subject = match (phone, email) {
        (Some(phone), _) => format!("phone:{phone}"),
        (None, Some(email)) => format!("email:{email}"),
        (None, None) => unreachable!("an account always has a phone number or an e-mail address"),
    };
    // Two first posts at once: one insert makes it, and the other finds it.
    let created = sqlx::query_as!(
        Account,
        "INSERT INTO users (auth_subject, phone_e164, email, display_name, status)
         VALUES ($1, $2::text::e164_phone, $3::text::citext, coalesce($4, 'Përdorues'), 'ACTIVE')
         ON CONFLICT DO NOTHING
         RETURNING id, status",
        subject,
        phone,
        email,
        name,
    )
    .fetch_optional(db)
    .await?;
    match created {
        Some(account) => Ok(account),
        None => account_by_contact(db, phone, email).await?.ok_or(sqlx::Error::RowNotFound),
    }
}

/// Publishes a listing posted before signing in, once its seller confirmed their contact: a draft,
/// or a new account's first listing waiting for a moderator only for that (review.rs,
/// NEW_ACCOUNT); a draft with a photo another seller's listing has goes on to wait for them
/// (DUPLICATE_PHOTOS). Returns its number and slug, for its address (format::listing_path).
pub async fn publish_draft(db: &PgPool, listing_id: Uuid, user_id: Uuid) -> sqlx::Result<Option<(i64, String)>> {
    let published = sqlx::query!(
        r#"UPDATE listings
           SET status = CASE WHEN review_reason = 'DUPLICATE_PHOTOS' THEN 'PENDING_REVIEW' ELSE 'ACTIVE' END,
               published_at = CASE WHEN review_reason = 'DUPLICATE_PHOTOS' THEN NULL ELSE now() END,
               expires_at = CASE WHEN review_reason = 'DUPLICATE_PHOTOS' THEN NULL ELSE now() + make_interval(days => $3) END,
               review_reason = CASE WHEN review_reason = 'DUPLICATE_PHOTOS' THEN review_reason END
           WHERE id = $1 AND created_by_user_id = $2
             AND (status = 'DRAFT' OR (status = 'PENDING_REVIEW' AND review_reason = 'NEW_ACCOUNT'))
           RETURNING public_no, slug"#,
        listing_id,
        user_id,
        super::LISTING_DAYS,
    )
    .fetch_optional(db)
    .await?;
    Ok(published.map(|listing| (listing.public_no, listing.slug)))
}

/// A session for the user, `days` long, its token kept as its hash, made by `client`: the pages
/// ("web", which name no device) or the phone app ("ios", "android", with the phone's name the
/// app gives, `device_name`), as migration 0043 has them.
pub async fn create_session(
    db: &PgPool,
    token_hash: &[u8],
    user_id: Uuid,
    days: i32,
    client: &str,
    device_name: Option<&str>,
) -> sqlx::Result<()> {
    sqlx::query!(
        "INSERT INTO sessions (token_hash, user_id, expires_at, client, device_name)
         VALUES ($1, $2, now() + make_interval(days => $3), $4, $5)",
        token_hash,
        user_id,
        days,
        client,
        device_name,
    )
    .execute(db)
    .await
    .map(|_| ())
}

/// The signed-in user, with what they signed in with.
pub struct SessionUser {
    pub id: Uuid,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub display_name: String,
}

/// The user a session token belongs to, unless it expired or the account is closed or suspended:
/// the pages' cookie's, or the phone app's bearer token's (pluribus-api), one table for both.
pub async fn session_user(db: &PgPool, token_hash: &[u8]) -> sqlx::Result<Option<SessionUser>> {
    // In the same statement, when they were last on the site (users.last_seen_at, which their
    // listings' seller card says, "Aktiv sot") and when this session was last used
    // (sessions.last_used_at, migration 0043, for the devices signed in), each moved to now once it
    // is ten minutes old: a write now and then, not on every page or call.
    sqlx::query_as!(
        SessionUser,
        r#"WITH me AS (
               SELECT u.id, u.phone_e164::text AS phone, u.email::text AS email, u.display_name, u.last_seen_at,
                      s.last_used_at
               FROM sessions s
               JOIN users u ON u.id = s.user_id
               WHERE s.token_hash = $1 AND s.expires_at > now() AND u.status = 'ACTIVE'
           ), seen AS (
               UPDATE users u SET last_seen_at = now()
               FROM me
               WHERE u.id = me.id AND (me.last_seen_at IS NULL OR me.last_seen_at < now() - interval '10 minutes')
           ), used AS (
               UPDATE sessions s SET last_used_at = now()
               FROM me
               WHERE s.token_hash = $1 AND (me.last_used_at IS NULL OR me.last_used_at < now() - interval '10 minutes')
           )
           SELECT me.id AS "id!", me.phone, me.email, me.display_name AS "display_name!" FROM me"#,
        token_hash,
    )
    .fetch_optional(db)
    .await
}

pub async fn delete_session(db: &PgPool, token_hash: &[u8]) -> sqlx::Result<()> {
    sqlx::query!("DELETE FROM sessions WHERE token_hash = $1", token_hash).execute(db).await.map(|_| ())
}
