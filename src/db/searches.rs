//! Saved searches (web/searches.rs): a user's searches, each a page and its filters kept to be
//! opened again (`filters`: {"path", "query"}), how many listings came since it was last opened,
//! and the alerts of new ones (notifications, SAVED_SEARCH).

use super::*;

/// The most searches one user keeps.
pub const SEARCHES_MOST: i64 = 30;

/// One of a user's saved searches.
#[derive(Debug, sqlx::FromRow)]
pub struct SavedSearch {
    pub id: Uuid,
    pub name: String,
    /// The page it searches (/c/makina, /search), and its filters as a query string.
    pub path: String,
    pub query: String,
    pub alerts_enabled: bool,
    /// Since when its new listings count: when it was last opened from /kerkimet-e-ruajtura, or
    /// saved.
    pub seen_since: OffsetDateTime,
}

/// The search of this page and query that the user keeps, if they keep it.
pub async fn saved_search_id(db: &PgPool, user_id: Uuid, path: &str, query: &str) -> sqlx::Result<Option<Uuid>> {
    sqlx::query_scalar(
        "SELECT id FROM saved_searches
         WHERE user_id = $1 AND filters = jsonb_build_object('path', $2::text, 'query', $3::text)",
    )
    .bind(user_id)
    .bind(path)
    .bind(query)
    .fetch_optional(db)
    .await
}

/// A search to keep: its page and query, the name it shows by, its category and words.
pub struct NewSearch<'a> {
    pub name: &'a str,
    pub category_id: Option<Uuid>,
    pub words: Option<&'a str>,
    pub path: &'a str,
    pub query: &'a str,
}

/// Keeps a search for the user, or finds the one they keep already: its id. None when they keep
/// SEARCHES_MOST already.
pub async fn save_search(db: &PgPool, user_id: Uuid, search: &NewSearch<'_>) -> sqlx::Result<Option<Uuid>> {
    if let Some(id) = saved_search_id(db, user_id, search.path, search.query).await? {
        return Ok(Some(id));
    }
    let kept: i64 = sqlx::query_scalar("SELECT count(*) FROM saved_searches WHERE user_id = $1")
        .bind(user_id)
        .fetch_one(db)
        .await?;
    if kept >= SEARCHES_MOST {
        return Ok(None);
    }
    sqlx::query_scalar(
        "INSERT INTO saved_searches (user_id, name, category_id, query_text, filters)
         VALUES ($1, left($2, 120), $3, left($4, 300), jsonb_build_object('path', $5::text, 'query', $6::text))
         ON CONFLICT (user_id, filters) DO UPDATE SET updated_at = now()
         RETURNING id",
    )
    .bind(user_id)
    .bind(search.name)
    .bind(search.category_id)
    .bind(search.words)
    .bind(search.path)
    .bind(search.query)
    .fetch_one(db)
    .await
    .map(Some)
}

/// Lets one of the user's searches go; false if it isn't theirs.
pub async fn delete_search(db: &PgPool, user_id: Uuid, id: Uuid) -> sqlx::Result<bool> {
    let deleted = sqlx::query("DELETE FROM saved_searches WHERE id = $1 AND user_id = $2")
        .bind(id)
        .bind(user_id)
        .execute(db)
        .await?
        .rows_affected();
    Ok(deleted == 1)
}

/// The user's searches, the last saved first.
pub async fn saved_searches(db: &PgPool, user_id: Uuid) -> sqlx::Result<Vec<SavedSearch>> {
    sqlx::query_as(
        "SELECT id, name, filters ->> 'path' AS path, filters ->> 'query' AS query, alerts_enabled,
                coalesce(last_seen_at, created_at) AS seen_since
         FROM saved_searches
         WHERE user_id = $1
         ORDER BY created_at DESC, id DESC
         LIMIT $2",
    )
    .bind(user_id)
    .bind(SEARCHES_MOST)
    .fetch_all(db)
    .await
}

/// The user opens one of their searches: its new listings count from now on, and its alerts are
/// seen. Its page and query, or None if it isn't theirs.
pub async fn open_search(db: &PgPool, user_id: Uuid, id: Uuid) -> sqlx::Result<Option<(String, String)>> {
    let opened: Option<(String, String)> = sqlx::query_as(
        "UPDATE saved_searches SET last_seen_at = now()
         WHERE id = $1 AND user_id = $2
         RETURNING filters ->> 'path', filters ->> 'query'",
    )
    .bind(id)
    .bind(user_id)
    .fetch_optional(db)
    .await?;
    if opened.is_some() {
        sqlx::query(
            "UPDATE notifications SET read_at = now()
             WHERE user_id = $1 AND notification_type = 'SAVED_SEARCH' AND read_at IS NULL
               AND payload ->> 'search' = $2::text",
        )
        .bind(user_id)
        .bind(id.to_string())
        .execute(db)
        .await?;
    }
    Ok(opened)
}

/// How many of the user's saved searches have alerts they haven't seen, for the heart's dot
/// (heart_controller.js) and the link on /te-ruajturat.
pub async fn unseen_search_alerts(db: &PgPool, user_id: Uuid) -> sqlx::Result<i64> {
    sqlx::query_scalar(
        "SELECT count(DISTINCT payload ->> 'search') FROM notifications
         WHERE user_id = $1 AND notification_type = 'SAVED_SEARCH' AND read_at IS NULL",
    )
    .bind(user_id)
    .fetch_one(db)
    .await
}

/// The user saw their saved searches, with how many new listings each has: their alerts are seen.
pub async fn search_alerts_seen(db: &PgPool, user_id: Uuid) -> sqlx::Result<()> {
    sqlx::query(
        "UPDATE notifications SET read_at = now()
         WHERE user_id = $1 AND notification_type = 'SAVED_SEARCH' AND read_at IS NULL",
    )
    .bind(user_id)
    .execute(db)
    .await
    .map(|_| ())
}

/// Turns the alerts of one of the user's searches on or off; false if it isn't theirs.
pub async fn set_search_alerts(db: &PgPool, user_id: Uuid, id: Uuid, on: bool) -> sqlx::Result<bool> {
    let changed = sqlx::query("UPDATE saved_searches SET alerts_enabled = $3 WHERE id = $1 AND user_id = $2")
        .bind(id)
        .bind(user_id)
        .bind(on)
        .execute(db)
        .await?
        .rows_affected();
    Ok(changed == 1)
}

/// A saved search whose alert of new listings may be due: since when they count, and how to reach
/// its saver.
#[derive(Debug, sqlx::FromRow)]
pub struct DueSearchAlert {
    pub id: Uuid,
    pub user_id: Uuid,
    pub name: String,
    pub path: String,
    pub query: String,
    /// Its last alert, or when it was saved: listings newer than that are new.
    pub since: OffsetDateTime,
    pub email: Option<String>,
    pub phone: Option<String>,
    /// Whether its saver lets the site e-mail them (notification_preferences.email_enabled).
    pub email_ok: bool,
    /// The language their e-mail is in (users.preferred_language: "sq", "en").
    pub language: String,
}

/// The searches with alerts on whose last alert, or saving, is at least `gap` ago, of active users
/// who want such alerts (notification_preferences.saved_search_alerts, on unless a row says
/// otherwise): at most 200, those waiting longest first.
pub async fn due_search_alerts(db: &PgPool, gap: std::time::Duration) -> sqlx::Result<Vec<DueSearchAlert>> {
    sqlx::query_as(
        "SELECT s.id, s.user_id, s.name, s.filters ->> 'path' AS path, s.filters ->> 'query' AS query,
                coalesce(s.last_alerted_at, s.created_at) AS since,
                u.email::text AS email, u.phone_e164::text AS phone, coalesce(np.email_enabled, true) AS email_ok,
                u.preferred_language AS language
         FROM saved_searches s
         JOIN users u ON u.id = s.user_id AND u.status = 'ACTIVE'
         LEFT JOIN notification_preferences np ON np.user_id = s.user_id
         WHERE s.alerts_enabled AND coalesce(np.saved_search_alerts, true)
           AND coalesce(s.last_alerted_at, s.created_at) < now() - make_interval(secs => $1)
         ORDER BY coalesce(s.last_alerted_at, s.created_at), s.id
         LIMIT 200",
    )
    .bind(gap.as_secs_f64())
    .fetch_all(db)
    .await
}

/// Records an alert of a search's new listings: a notification (SAVED_SEARCH, with the search,
/// how many and where to see them), and when it went out, which the next one counts from.
pub async fn record_search_alert(
    db: &PgPool,
    user_id: Uuid,
    search_id: Uuid,
    title: &str,
    count: i64,
    url: &str,
) -> sqlx::Result<()> {
    let mut tx = db.begin().await?;
    sqlx::query(
        "INSERT INTO notifications (user_id, notification_type, title, payload)
         VALUES ($1, 'SAVED_SEARCH', left($2, 180), jsonb_build_object('search', $3::text, 'count', $4::bigint, 'url', $5::text))",
    )
    .bind(user_id)
    .bind(title)
    .bind(search_id.to_string())
    .bind(count)
    .bind(url)
    .execute(&mut *tx)
    .await?;
    sqlx::query("UPDATE saved_searches SET last_alerted_at = now() WHERE id = $1")
        .bind(search_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await
}
