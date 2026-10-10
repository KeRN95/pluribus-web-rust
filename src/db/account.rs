//! The account's own settings ("Cilësimet", web/settings.rs): its name, its phone number and its
//! e-mail address, what it hears of, the devices signed in to it, and closing it, as README's
//! "Users are anonymised, not deleted" has it.

use crate::format::Contact;

use super::*;

/// The account as its settings show it.
#[derive(Debug, sqlx::FromRow)]
pub struct AccountSettings {
    pub display_name: String,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub created_at: OffsetDateTime,
}

pub async fn account_settings(db: &PgPool, user_id: Uuid) -> sqlx::Result<AccountSettings> {
    sqlx::query_as("SELECT display_name, phone_e164::text AS phone, email::text AS email, created_at FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_one(db)
        .await
}

/// The name the account shows by: on its listings, its profile, in its conversations.
pub async fn rename_account(db: &PgPool, user_id: Uuid, name: &str) -> sqlx::Result<()> {
    sqlx::query("UPDATE users SET display_name = $2, updated_at = now() WHERE id = $1")
        .bind(user_id)
        .bind(name)
        .execute(db)
        .await?;
    Ok(())
}

/// What the account hears of, and whether by e-mail (notification_preferences): the messages it
/// left unread (notices.rs), the prices dropped of the listings it keeps (price_drops.rs), the
/// saved searches' new listings (searches.rs). All of them, until it says otherwise.
#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::FromRow)]
pub struct NoticePreferences {
    pub message_alerts: bool,
    pub price_drop_alerts: bool,
    pub saved_search_alerts: bool,
    pub email_enabled: bool,
}

impl Default for NoticePreferences {
    fn default() -> Self {
        Self { message_alerts: true, price_drop_alerts: true, saved_search_alerts: true, email_enabled: true }
    }
}

pub async fn notice_preferences(db: &PgPool, user_id: Uuid) -> sqlx::Result<NoticePreferences> {
    let kept = sqlx::query_as(
        "SELECT message_alerts, price_drop_alerts, saved_search_alerts, email_enabled
         FROM notification_preferences WHERE user_id = $1",
    )
    .bind(user_id)
    .fetch_optional(db)
    .await?;
    Ok(kept.unwrap_or_default())
}

pub async fn set_notice_preferences(db: &PgPool, user_id: Uuid, preferences: NoticePreferences) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO notification_preferences (user_id, message_alerts, price_drop_alerts, saved_search_alerts, email_enabled)
         VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (user_id) DO UPDATE
         SET message_alerts = EXCLUDED.message_alerts, price_drop_alerts = EXCLUDED.price_drop_alerts,
             saved_search_alerts = EXCLUDED.saved_search_alerts, email_enabled = EXCLUDED.email_enabled, updated_at = now()",
    )
    .bind(user_id)
    .bind(preferences.message_alerts)
    .bind(preferences.price_drop_alerts)
    .bind(preferences.saved_search_alerts)
    .bind(preferences.email_enabled)
    .execute(db)
    .await?;
    Ok(())
}

/// Puts a contact its code just confirmed on the account, in place of the one of its kind it had,
/// and on the account's listings that showed that one. The account's identity (auth_subject) stops
/// being a contact, which someone else may sign in with one day. False when another account has
/// the contact (taken since the code was asked for).
pub async fn set_contact(db: &PgPool, user_id: Uuid, contact: &Contact) -> sqlx::Result<bool> {
    let mut tx = db.begin().await?;
    let (old_phone, old_email): (Option<String>, Option<String>) =
        sqlx::query_as("SELECT phone_e164::text, email::text FROM users WHERE id = $1 FOR UPDATE")
            .bind(user_id)
            .fetch_one(&mut *tx)
            .await?;
    let taken: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM users WHERE id <> $1 AND status <> 'DELETED'
                        AND (phone_e164 = $2::text::e164_phone OR email = $3::text::citext))",
    )
    .bind(user_id)
    .bind(contact.phone())
    .bind(contact.email())
    .fetch_one(&mut *tx)
    .await?;
    if taken {
        return Ok(false);
    }
    match contact {
        Contact::Phone(phone) => {
            sqlx::query("UPDATE users SET phone_e164 = $2::text::e164_phone, auth_subject = 'user:' || id::text, updated_at = now() WHERE id = $1")
                .bind(user_id)
                .bind(phone)
                .execute(&mut *tx)
                .await?;
            if let Some(old) = old_phone.filter(|old| old != phone) {
                sqlx::query(
                    "UPDATE listing_contacts c SET phone_e164 = $3::text::e164_phone
                     FROM listings l WHERE l.id = c.listing_id AND l.created_by_user_id = $1 AND c.phone_e164 = $2::text::e164_phone",
                )
                .bind(user_id)
                .bind(old)
                .bind(phone)
                .execute(&mut *tx)
                .await?;
            }
        }
        Contact::Email(email) => {
            sqlx::query("UPDATE users SET email = $2::text::citext, auth_subject = 'user:' || id::text, updated_at = now() WHERE id = $1")
                .bind(user_id)
                .bind(email)
                .execute(&mut *tx)
                .await?;
            if let Some(old) = old_email.filter(|old| !old.eq_ignore_ascii_case(email)) {
                sqlx::query(
                    "UPDATE listing_contacts c SET email = $3::text::citext
                     FROM listings l WHERE l.id = c.listing_id AND l.created_by_user_id = $1 AND c.email = $2::text::citext",
                )
                .bind(user_id)
                .bind(old)
                .bind(email)
                .execute(&mut *tx)
                .await?;
            }
        }
    }
    tx.commit().await?;
    Ok(true)
}

/// Takes the account's phone number (`phone`) or its e-mail address off it, which keeps the other:
/// its listings that showed the one taken off show the other instead. False when it is the only
/// one the account has, which signing in needs.
pub async fn remove_contact(db: &PgPool, user_id: Uuid, phone: bool) -> sqlx::Result<bool> {
    let mut tx = db.begin().await?;
    let (old_phone, old_email): (Option<String>, Option<String>) =
        sqlx::query_as("SELECT phone_e164::text, email::text FROM users WHERE id = $1 FOR UPDATE")
            .bind(user_id)
            .fetch_one(&mut *tx)
            .await?;
    let (Some(old_phone), Some(old_email)) = (old_phone, old_email) else {
        return Ok(false);
    };
    if phone {
        sqlx::query("UPDATE users SET phone_e164 = NULL, auth_subject = 'user:' || id::text, updated_at = now() WHERE id = $1")
            .bind(user_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "UPDATE listing_contacts c SET phone_e164 = NULL, email = coalesce(c.email, $3::text::citext), show_email = true
             FROM listings l WHERE l.id = c.listing_id AND l.created_by_user_id = $1 AND c.phone_e164 = $2::text::e164_phone",
        )
        .bind(user_id)
        .bind(&old_phone)
        .bind(&old_email)
        .execute(&mut *tx)
        .await?;
    } else {
        sqlx::query("UPDATE users SET email = NULL, auth_subject = 'user:' || id::text, updated_at = now() WHERE id = $1")
            .bind(user_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "UPDATE listing_contacts c SET email = NULL, phone_e164 = coalesce(c.phone_e164, $3::text::e164_phone), show_phone = true
             FROM listings l WHERE l.id = c.listing_id AND l.created_by_user_id = $1 AND c.email = $2::text::citext",
        )
        .bind(user_id)
        .bind(&old_email)
        .bind(&old_phone)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(true)
}

/// How many devices are signed in to the account: its sessions still good.
pub async fn session_count(db: &PgPool, user_id: Uuid) -> sqlx::Result<i64> {
    sqlx::query_scalar("SELECT count(*) FROM sessions WHERE user_id = $1 AND expires_at > now()")
        .bind(user_id)
        .fetch_one(db)
        .await
}

/// Signs the account out everywhere but on this device (`keep`, its session's hash): how many
/// sessions ended.
pub async fn end_other_sessions(db: &PgPool, user_id: Uuid, keep: &[u8]) -> sqlx::Result<u64> {
    let ended = sqlx::query("DELETE FROM sessions WHERE user_id = $1 AND token_hash <> $2")
        .bind(user_id)
        .bind(keep)
        .execute(db)
        .await?;
    Ok(ended.rows_affected())
}

/// The name a closed account shows by, in the conversations the other side keeps.
pub const DELETED_NAME: &str = "Përdorues i fshirë";

/// Closes the account, in one transaction (README, "Users are anonymised, not deleted"): its
/// listings deleted (the status history saying so), their contacts and photos with them; what only
/// it had gone (sessions, notifications and what it hears of, saved listings and searches, blocks
/// either way, verifications, business memberships, codes asked for its contacts); and the account
/// itself anonymised, no phone number, address or picture, DELETED_NAME, the identity it signed in
/// with freed. Messages stay for the other side, under DELETED_NAME; reports and moderation stay.
/// Returns the photos' files (listing_media.storage_key), which the caller takes out of the store
/// once this is done.
pub async fn delete_account(db: &PgPool, user_id: Uuid) -> sqlx::Result<Vec<String>> {
    let mut tx = db.begin().await?;
    sqlx::query("SELECT set_config('app.user_id', $1, true), set_config('app.status_reason', 'Llogaria u mbyll', true)")
        .bind(user_id.to_string())
        .execute(&mut *tx)
        .await?;
    let photos: Vec<String> = sqlx::query_scalar(
        "SELECT m.storage_key FROM listing_media m JOIN listings l ON l.id = m.listing_id WHERE l.created_by_user_id = $1",
    )
    .bind(user_id)
    .fetch_all(&mut *tx)
    .await?;
    for statement in [
        "DELETE FROM listing_media m USING listings l WHERE l.id = m.listing_id AND l.created_by_user_id = $1",
        "DELETE FROM listing_contacts c USING listings l WHERE l.id = c.listing_id AND l.created_by_user_id = $1",
        "UPDATE listings SET status = 'DELETED' WHERE created_by_user_id = $1 AND status <> 'DELETED'",
        "DELETE FROM login_codes
         WHERE user_id = $1 OR contact IN (SELECT phone_e164::text FROM users WHERE id = $1 UNION SELECT email::text FROM users WHERE id = $1)",
        "DELETE FROM sessions WHERE user_id = $1",
        "DELETE FROM device_tokens WHERE user_id = $1",
        "DELETE FROM notifications WHERE user_id = $1",
        "DELETE FROM notification_preferences WHERE user_id = $1",
        "DELETE FROM saved_searches WHERE user_id = $1",
        "DELETE FROM saved_listings WHERE user_id = $1",
        "DELETE FROM favorites WHERE user_id = $1",
        "DELETE FROM user_verifications WHERE user_id = $1",
        "DELETE FROM user_blocks WHERE blocker_user_id = $1 OR blocked_user_id = $1",
        "DELETE FROM business_members WHERE user_id = $1",
    ] {
        sqlx::query(statement).bind(user_id).execute(&mut *tx).await?;
    }
    sqlx::query(
        "UPDATE users SET email = NULL, phone_e164 = NULL, avatar_url = NULL, display_name = $2,
                          auth_subject = 'deleted:' || id::text, status = 'DELETED', deleted_at = now(), updated_at = now()
         WHERE id = $1",
    )
    .bind(user_id)
    .bind(DELETED_NAME)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(photos)
}
