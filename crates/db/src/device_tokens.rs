//! The phone app's devices, which push notifications go to (pluribus-services' push.rs): each
//! the token Firebase Cloud Messaging gave the app on it, for the account signed in there
//! (device_tokens, from migration 0001: a token once, its platform IOS, ANDROID or WEB, when it
//! was last seen). The app sends its token as it starts and whenever Firebase gives it another
//! (`PUT /me/devices`), and takes it back as its user signs out (`DELETE /me/devices/{token}`);
//! closing the account forgets its tokens with the rest (account.rs, delete_account).

use super::*;

/// Keeps `token`, the one Firebase gave the app on a device of `platform` (IOS or ANDROID), for
/// `user_id`, and sees it now (`last_seen_at`): a token kept already, which a phone that changed
/// accounts sends again, moves to this user, with its platform.
pub async fn save_device_token(db: &PgPool, user_id: Uuid, platform: &str, token: &str) -> sqlx::Result<()> {
    sqlx::query!(
        "INSERT INTO device_tokens (user_id, platform, token) VALUES ($1, $2, $3)
         ON CONFLICT (token) DO UPDATE SET user_id = EXCLUDED.user_id, platform = EXCLUDED.platform, last_seen_at = now()",
        user_id,
        platform,
        token,
    )
    .execute(db)
    .await
    .map(|_| ())
}

/// Forgets `token`: `user`'s alone where one is given (the app signing its user out), whoever's
/// where none is (Firebase no longer knows it, push.rs). Whether one was kept.
pub async fn forget_device_token(db: &PgPool, token: &str, user: Option<Uuid>) -> sqlx::Result<bool> {
    let forgotten = sqlx::query!("DELETE FROM device_tokens WHERE token = $1 AND ($2::uuid IS NULL OR user_id = $2)", token, user)
        .execute(db)
        .await?;
    Ok(forgotten.rows_affected() > 0)
}

/// The user's devices, each its platform and its token, the one seen last first; none while the
/// account turns push notifications off (notification_preferences.push_enabled, on until a row
/// says otherwise, as the schema has it).
pub async fn device_tokens(db: &PgPool, user_id: Uuid) -> sqlx::Result<Vec<(String, String)>> {
    let rows = sqlx::query!(
        "SELECT d.platform, d.token
         FROM device_tokens d
         WHERE d.user_id = $1
           AND coalesce((SELECT np.push_enabled FROM notification_preferences np WHERE np.user_id = $1), true)
         ORDER BY d.last_seen_at DESC, d.id",
        user_id,
    )
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().map(|row| (row.platform, row.token)).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[sqlx::test(migrator = "crate::MIGRATOR")]
    async fn a_token_is_kept_once_moves_with_its_phone_and_is_forgotten(db: PgPool) -> sqlx::Result<()> {
        let user = |phone: &'static str| {
            sqlx::query_scalar::<_, Uuid>("INSERT INTO users (auth_subject, phone_e164, display_name) VALUES ('phone:' || $1, $1, 'Ana') RETURNING id")
                .bind(phone)
                .fetch_one(&db)
        };
        let (ana, besa) = (user("+355690000001").await?, user("+355690000002").await?);
        save_device_token(&db, ana, "ANDROID", "fcm-1").await?;
        save_device_token(&db, ana, "IOS", "fcm-2").await?;
        let seen = |token: &'static str| {
            sqlx::query_scalar::<_, OffsetDateTime>("SELECT last_seen_at FROM device_tokens WHERE token = $1").bind(token).fetch_one(&db)
        };
        sqlx::query("UPDATE device_tokens SET last_seen_at = now() - interval '1 day' WHERE token = 'fcm-1'").execute(&db).await?;
        let before = seen("fcm-1").await?;
        // Sent again as the app starts: seen now, the first again.
        save_device_token(&db, ana, "ANDROID", "fcm-1").await?;
        assert!(seen("fcm-1").await? > before);
        assert_eq!(device_tokens(&db, ana).await?, [("ANDROID".to_string(), "fcm-1".to_string()), ("IOS".into(), "fcm-2".into())]);
        // The iPhone signed in to Besa's account: the token is hers now, not Ana's as well.
        save_device_token(&db, besa, "IOS", "fcm-2").await?;
        assert_eq!(device_tokens(&db, ana).await?, [("ANDROID".to_string(), "fcm-1".to_string())]);
        assert_eq!(device_tokens(&db, besa).await?, [("IOS".to_string(), "fcm-2".to_string())]);
        // Turned off, none; on again, hers again.
        sqlx::query("INSERT INTO notification_preferences (user_id, push_enabled) VALUES ($1, false)").bind(besa).execute(&db).await?;
        assert!(device_tokens(&db, besa).await?.is_empty());
        sqlx::query("UPDATE notification_preferences SET push_enabled = true WHERE user_id = $1").bind(besa).execute(&db).await?;
        assert_eq!(device_tokens(&db, besa).await?.len(), 1);
        // Ana can't forget Besa's; Besa can, and Firebase any.
        assert!(!forget_device_token(&db, "fcm-2", Some(ana)).await?);
        assert!(forget_device_token(&db, "fcm-2", Some(besa)).await?);
        assert!(forget_device_token(&db, "fcm-1", None).await?);
        assert!(!forget_device_token(&db, "fcm-1", None).await?);
        assert!(device_tokens(&db, ana).await?.is_empty() && device_tokens(&db, besa).await?.is_empty());
        Ok(())
    }
}
