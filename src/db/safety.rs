//! Trust and safety: reports of listings ("Raporto"), which take a listing off the site once enough
//! people report it for a serious reason, until a moderator keeps it or takes it down (/moderim);
//! blocks, a signed-in user's way to stop seeing a seller's listings; and the contacts people
//! proved theirs by signing in with a code, which listings and profiles say are verified.

use super::*;

/// What a report says is wrong, by the codes reports keep (reports.reason_code), in the order the
/// form offers them. The first four are serious (SERIOUS_REASONS).
pub const REASONS: [&str; 7] = ["SCAM", "FAKE", "PROHIBITED", "SPAM", "UNAVAILABLE", "WRONG_DETAILS", "OTHER"];

/// The reasons that count towards taking a listing off the site: a listing three people say has
/// the wrong category is for a moderator to look at, not to hide.
pub const SERIOUS_REASONS: [&str; 4] = ["SCAM", "FAKE", "PROHIBITED", "SPAM"];

/// How many people reporting a listing for a serious reason take it off the site (SUSPENDED),
/// until a moderator decides: enough that one person, or one with a second account, can't.
pub const HIDE_AFTER: i64 = 3;

/// How many listings one person may report in a day.
pub const REPORTS_PER_DAY: i64 = 20;

/// What came of a report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reported {
    /// Kept for the moderators; `hidden` when it was the one that took the listing off the site.
    Taken { hidden: bool },
    /// This person reported the listing already, and no moderator has decided on it since.
    Already,
    /// No such listing on the site, or it is the reporter's own.
    NotReportable,
}

/// Reports a listing, at most once a person until a moderator decides (ux_reports_open_per_reporter).
/// When this makes HIDE_AFTER people reporting it for a serious reason, the listing comes off the
/// site until a moderator decides: its status history says why, and moderation_actions keeps it
/// as the rule's doing (no moderator), with the status it had to go back to.
pub async fn report_listing(
    db: &PgPool,
    public_no: i64,
    reporter: Uuid,
    reason: &str,
    details: Option<&str>,
) -> sqlx::Result<Reported> {
    let mut tx = db.begin().await?;
    // Locked, so a listing's reports are counted one after the other.
    let Some((id, owner, status)) = sqlx::query_as::<_, (Uuid, Uuid, String)>(
        "SELECT id, created_by_user_id, status FROM listings WHERE public_no = $1 FOR UPDATE",
    )
    .bind(public_no)
    .fetch_optional(&mut *tx)
    .await?
    else {
        return Ok(Reported::NotReportable);
    };
    if owner == reporter || !matches!(status.as_str(), "ACTIVE" | "RESERVED") {
        return Ok(Reported::NotReportable);
    }
    let taken = sqlx::query(
        "INSERT INTO reports (reporter_user_id, listing_id, reported_user_id, reason_code, details)
         VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (reporter_user_id, listing_id) WHERE status IN ('OPEN', 'IN_REVIEW') DO NOTHING",
    )
    .bind(reporter)
    .bind(id)
    .bind(owner)
    .bind(reason)
    .bind(details)
    .execute(&mut *tx)
    .await?
    .rows_affected()
        == 1;
    if !taken {
        return Ok(Reported::Already);
    }
    let reporters: i64 = sqlx::query_scalar(
        "SELECT count(DISTINCT reporter_user_id) FROM reports
         WHERE listing_id = $1 AND status = 'OPEN' AND reason_code = ANY($2)",
    )
    .bind(id)
    .bind(&SERIOUS_REASONS[..])
    .fetch_one(&mut *tx)
    .await?;
    let hidden = reporters >= HIDE_AFTER;
    if hidden {
        sqlx::query("SELECT set_config('app.status_reason', 'Fshehur pas raportimeve', true)").execute(&mut *tx).await?;
        sqlx::query("UPDATE listings SET status = 'SUSPENDED', updated_at = now() WHERE id = $1")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "INSERT INTO moderation_actions (listing_id, target_user_id, action_type, reason, metadata)
             VALUES ($1, $2, 'SUSPEND_LISTING', 'reports', jsonb_build_object('from', $3::text, 'reporters', $4::bigint))",
        )
        .bind(id)
        .bind(owner)
        .bind(&status)
        .bind(reporters)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(Reported::Taken { hidden })
}

/// How many listings this person reported over the last day (REPORTS_PER_DAY).
pub async fn reports_today(db: &PgPool, reporter: Uuid) -> sqlx::Result<i64> {
    sqlx::query_scalar("SELECT count(*) FROM reports WHERE reporter_user_id = $1 AND created_at > now() - interval '1 day'")
        .bind(reporter)
        .fetch_one(db)
        .await
}

/// Whether this person has a report of the listing that no moderator has decided on yet.
pub async fn has_reported(db: &PgPool, public_no: i64, reporter: Uuid) -> sqlx::Result<bool> {
    sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM reports r JOIN listings l ON l.id = r.listing_id
                        WHERE l.public_no = $1 AND r.reporter_user_id = $2 AND r.status IN ('OPEN', 'IN_REVIEW'))",
    )
    .bind(public_no)
    .bind(reporter)
    .fetch_one(db)
    .await
}

/// A listing people reported, which waits for a moderator.
#[derive(Debug, sqlx::FromRow)]
pub struct ReportedListing {
    pub public_no: i64,
    pub slug: String,
    pub title: String,
    /// SUSPENDED when the reports took it off the site.
    pub status: String,
    pub seller_name: String,
    pub seller_url: String,
    /// The account that posted it, which a moderator may suspend.
    pub created_by_user_id: Uuid,
    /// How many people report it.
    pub reporters: i64,
    /// The reasons given (REASONS), the most given first, and how many gave each.
    pub reason_codes: Vec<String>,
    pub reason_counts: Vec<i64>,
    /// What they wrote, the latest first.
    pub notes: Vec<String>,
    pub last_reported: OffsetDateTime,
}

/// The listings with reports no moderator has decided on, those the reports took off the site
/// first, then the most reported; at most 100.
pub async fn reported_listings(db: &PgPool) -> sqlx::Result<Vec<ReportedListing>> {
    sqlx::query_as(
        "SELECT l.public_no, l.slug, coalesce(l.generated_title, l.title) AS title, l.status,
                coalesce(b.display_name, u.display_name, '') AS seller_name,
                CASE WHEN b.id IS NOT NULL THEN '/biznes/' || b.slug ELSE '/perdorues/' || l.seller_user_id END AS seller_url,
                l.created_by_user_id, r.reporters, reasons.codes AS reason_codes, reasons.counts AS reason_counts, r.notes, r.last_reported
         FROM (SELECT listing_id, count(DISTINCT reporter_user_id) AS reporters, max(created_at) AS last_reported,
                      coalesce(array_agg(details ORDER BY created_at DESC) FILTER (WHERE details IS NOT NULL), '{}') AS notes
               FROM reports
               WHERE status = 'OPEN' AND listing_id IS NOT NULL
               GROUP BY listing_id) r
         JOIN listings l ON l.id = r.listing_id AND l.status <> 'DELETED'
         LEFT JOIN users u ON u.id = l.seller_user_id
         LEFT JOIN businesses b ON b.id = l.seller_business_id
         CROSS JOIN LATERAL (
             SELECT array_agg(given.reason_code ORDER BY given.n DESC, given.reason_code) AS codes,
                    array_agg(given.n ORDER BY given.n DESC, given.reason_code) AS counts
             FROM (SELECT reason_code, count(*) AS n FROM reports
                   WHERE listing_id = l.id AND status = 'OPEN'
                   GROUP BY reason_code) given
         ) reasons
         ORDER BY l.status = 'SUSPENDED' DESC, r.reporters DESC, r.last_reported DESC
         LIMIT 100",
    )
    .fetch_all(db)
    .await
}

/// How many listings wait for a moderator for their reports, for the link to /moderim.
pub async fn reported_count(db: &PgPool) -> sqlx::Result<i64> {
    sqlx::query_scalar(
        "SELECT count(DISTINCT r.listing_id) FROM reports r JOIN listings l ON l.id = r.listing_id
         WHERE r.status = 'OPEN' AND l.status <> 'DELETED'",
    )
    .fetch_one(db)
    .await
}

/// A moderator's decision on a reported listing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// Nothing wrong with it: its reports are dismissed, and if they took it off the site it goes
    /// back up as it was.
    Keep,
    /// It breaks the rules: off the site for good (REJECTED), its reports resolved.
    Remove,
}

/// A moderator decides on a listing's open reports, which close; the status history and
/// moderation_actions say who did what. False when it has none: none came, or someone decided.
pub async fn decide(db: &PgPool, public_no: i64, moderator: Uuid, decision: Decision) -> sqlx::Result<bool> {
    let mut tx = db.begin().await?;
    let Some((id, owner, status)) = sqlx::query_as::<_, (Uuid, Uuid, String)>(
        "SELECT id, created_by_user_id, status FROM listings WHERE public_no = $1 FOR UPDATE",
    )
    .bind(public_no)
    .fetch_optional(&mut *tx)
    .await?
    else {
        return Ok(false);
    };
    let (closed_as, resolution) = match decision {
        Decision::Keep => ("DISMISSED", "Mbajtur"),
        Decision::Remove => ("RESOLVED", "Hequr"),
    };
    let reports = sqlx::query(
        "UPDATE reports SET status = $2, resolution = $3, resolved_at = now(), assigned_to_user_id = $4
         WHERE listing_id = $1 AND status = 'OPEN'",
    )
    .bind(id)
    .bind(closed_as)
    .bind(resolution)
    .bind(moderator)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if reports == 0 {
        return Ok(false);
    }
    sqlx::query("SELECT set_config('app.user_id', $1, true), set_config('app.status_reason', $2, true)")
        .bind(moderator.to_string())
        .bind(match decision {
            Decision::Keep => "Mbajtur nga moderatorët",
            Decision::Remove => "Hequr nga moderatorët",
        })
        .execute(&mut *tx)
        .await?;
    let action = match (decision, status.as_str()) {
        (Decision::Keep, "SUSPENDED") => {
            // Back as it was before the reports took it off: active, or reserved.
            let was: Option<String> = sqlx::query_scalar(
                "SELECT metadata->>'from' FROM moderation_actions
                 WHERE listing_id = $1 AND action_type = 'SUSPEND_LISTING' AND moderator_user_id IS NULL
                 ORDER BY created_at DESC LIMIT 1",
            )
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?
            .flatten();
            let back = was.filter(|was| was == "RESERVED").unwrap_or_else(|| "ACTIVE".to_string());
            sqlx::query("UPDATE listings SET status = $2, updated_at = now() WHERE id = $1")
                .bind(id)
                .bind(back)
                .execute(&mut *tx)
                .await?;
            "RESTORE_LISTING"
        }
        (Decision::Keep, _) => "APPROVE_LISTING",
        (Decision::Remove, _) => {
            sqlx::query("UPDATE listings SET status = 'REJECTED', updated_at = now() WHERE id = $1 AND status <> 'DELETED'")
                .bind(id)
                .execute(&mut *tx)
                .await?;
            "REJECT_LISTING"
        }
    };
    sqlx::query(
        "INSERT INTO moderation_actions (moderator_user_id, listing_id, target_user_id, action_type, reason, metadata)
         VALUES ($1, $2, $3, $4, 'reports', jsonb_build_object('reports', $5::bigint))",
    )
    .bind(moderator)
    .bind(id)
    .bind(owner)
    .bind(action)
    .bind(reports as i64)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(true)
}

/// Blocks, or lets be again, the account behind a listing for this user: its listings no longer
/// show on their pages, but for its own (its profile, a listing's page they open), and neither can
/// write to the other (messages.rs, whose block in a conversation is this same one). False for
/// their own listing, or none.
pub async fn set_blocked(db: &PgPool, user: Uuid, public_no: i64, blocked: bool) -> sqlx::Result<bool> {
    let Some(owner) = sqlx::query_scalar::<_, Uuid>("SELECT created_by_user_id FROM listings WHERE public_no = $1")
        .bind(public_no)
        .fetch_optional(db)
        .await?
        .filter(|owner| *owner != user)
    else {
        return Ok(false);
    };
    let query = match blocked {
        true => "INSERT INTO user_blocks (blocker_user_id, blocked_user_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
        false => "DELETE FROM user_blocks WHERE blocker_user_id = $1 AND blocked_user_id = $2",
    };
    sqlx::query(query).bind(user).bind(owner).execute(db).await?;
    Ok(true)
}

/// Lets an account the user blocked be again ("Njoftimet e mia").
pub async fn unblock(db: &PgPool, user: Uuid, blocked: Uuid) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM user_blocks WHERE blocker_user_id = $1 AND blocked_user_id = $2")
        .bind(user)
        .bind(blocked)
        .execute(db)
        .await?;
    Ok(())
}

/// The accounts blocked by whoever this session is, whose listings the pages leave out for them:
/// one query, done only when there is a session.
pub async fn blocked_for_session(db: &PgPool, token_hash: &[u8]) -> sqlx::Result<Vec<Uuid>> {
    sqlx::query_scalar(
        "SELECT k.blocked_user_id
         FROM sessions s
         JOIN users u ON u.id = s.user_id AND u.status = 'ACTIVE'
         JOIN user_blocks k ON k.blocker_user_id = s.user_id
         WHERE s.token_hash = $1 AND s.expires_at > now()",
    )
    .bind(token_hash)
    .fetch_all(db)
    .await
}

/// Someone the user blocked, as their listings name them: a business's name for one of its people.
#[derive(Debug, sqlx::FromRow)]
pub struct BlockedSeller {
    pub id: Uuid,
    pub name: String,
    pub url: String,
}

/// Those the user blocked, on a listing's page or in a conversation, the last first.
pub async fn blocked_sellers(db: &PgPool, user: Uuid) -> sqlx::Result<Vec<BlockedSeller>> {
    sqlx::query_as(
        "SELECT u.id, coalesce(b.display_name, u.display_name) AS name,
                CASE WHEN b.id IS NOT NULL THEN '/biznes/' || b.slug ELSE '/perdorues/' || u.id END AS url
         FROM user_blocks k
         JOIN users u ON u.id = k.blocked_user_id
         LEFT JOIN LATERAL (SELECT b.id, b.display_name, b.slug
                            FROM business_members m JOIN businesses b ON b.id = m.business_id
                            WHERE m.user_id = u.id
                            ORDER BY m.role = 'OWNER' DESC
                            LIMIT 1) b ON true
         WHERE k.blocker_user_id = $1
         ORDER BY k.created_at DESC",
    )
    .bind(user)
    .fetch_all(db)
    .await
}

/// The contact someone signed in with, proved theirs by the code: their phone number or e-mail
/// address is verified (user_verifications), the one row of its kind, until it changes. A listing
/// showing it, and the profile, say so.
pub async fn verify_contact(db: &PgPool, user: Uuid, phone: Option<&str>, email: Option<&str>) -> sqlx::Result<()> {
    let (kind, value) = match (phone, email) {
        (Some(phone), _) => ("PHONE", phone),
        (None, Some(email)) => ("EMAIL", email),
        (None, None) => return Ok(()),
    };
    sqlx::query(
        "INSERT INTO user_verifications (user_id, verification_type, verified_value, status, provider, verified_at)
         VALUES ($1, $2, $3, 'VERIFIED', 'SIGN_IN_CODE', now())
         ON CONFLICT (user_id, verification_type) WHERE status = 'VERIFIED'
         DO UPDATE SET verified_value = EXCLUDED.verified_value, verified_at = now(), updated_at = now()",
    )
    .bind(user)
    .bind(kind)
    .bind(value)
    .execute(db)
    .await?;
    Ok(())
}

/// What the report page shows of a listing: its title and address, its first photo, its price and
/// its town (the card at the page's top), whether it is on the site to report, and whose it is.
#[derive(Debug, sqlx::FromRow)]
pub struct ReportTarget {
    pub public_no: i64,
    pub slug: String,
    pub title: String,
    pub status: String,
    pub created_by_user_id: Uuid,
    /// Its first photo's card-sized copy.
    pub photo: Option<String>,
    pub price_amount: Option<Decimal>,
    pub currency: String,
    pub price_type: String,
    pub listing_type: String,
    /// The municipality it is in, as its card names it.
    pub town: Option<String>,
}

impl ReportTarget {
    /// On the site, so there is something to report: active or reserved.
    pub fn reportable(&self) -> bool {
        matches!(self.status.as_str(), "ACTIVE" | "RESERVED")
    }
}

pub async fn report_target(db: &PgPool, public_no: i64) -> sqlx::Result<Option<ReportTarget>> {
    sqlx::query_as(
        "SELECT l.public_no, l.slug, coalesce(l.generated_title, l.title) AS title, l.status, l.created_by_user_id,
                (SELECT coalesce(m.thumbnail_url, m.public_url)
                 FROM listing_media m
                 WHERE m.listing_id = l.id AND m.status = 'READY' AND m.media_type = 'IMAGE'
                 ORDER BY m.is_cover DESC, m.sort_order
                 LIMIT 1) AS photo,
                l.price_amount, l.currency::text AS currency, l.price_type, l.listing_type,
                (SELECT t.name FROM locations t WHERE t.id = location_municipality(l.location_id)) AS town
         FROM listings l WHERE l.public_no = $1 AND l.status <> 'DELETED'",
    )
    .bind(public_no)
    .fetch_optional(db)
    .await
}
