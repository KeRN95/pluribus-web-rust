//! "Të rekomanduara për ty" (recommended.rs): the listings like those a visitor looked at, ranked
//! for all of them at once, and the listings a signed-in user showed an interest in.

use super::*;

/// How many of a visitor's seeds lead, the strongest (recommended.rs, Seeds): the listings like
/// each are looked for apart, so that more would cost more and add little.
const SOURCES: i64 = 16;

/// How many of the listings a signed-in user keeps, and of those they wrote to a seller about,
/// are seeds of their recommendations: the latest.
const KEPT_SEEDS: i64 = 12;
const WRITTEN_SEEDS: i64 = 6;

/// How long the ranking may take before the welcome page shows the newest instead: some 4 ms
/// for one seed and 35 for thirty, warm, against the dev data's million listings.
const RANKING_TIME: &str = "300ms";

/// The ids of the active listings recommended for a visitor who looked at these listings (their
/// numbers, each with its weight: Seeds::weighed), the most alike first, `limit` at most: as
/// "Njoftime të ngjashme" finds those like a listing (related_listings), for all of them at once.
/// The strongest SOURCES still on the site lead: for each, the candidates come cheaply off its
/// category's indexes (the 20 closest in price either way, the 20 nearest, the 20 newest of each
/// of their categories, and the 20 newest sharing a narrow choice, such as the model), and each
/// candidate is scored for every seed of its category, weighed by the seed: the choices shared
/// (a narrow one three, any other a half), the price (three at the same, none at half or
/// double), the place (two at the same, none 50 km away) and how new it is (one today, none two
/// months on). Left out: the seeds themselves, the `viewer`'s own listings (manages_listing),
/// those of the accounts they blocked (`hidden`), and a seller's past their two best. The
/// categories take turns, each as often as its seeds weigh among all of them, its best first:
/// the n-th of a category with a third of the weight stands where the 3n-th would of one with
/// all of it. Past RANKING_TIME (a busy database), none: the welcome page shows the newest.
pub async fn recommended_ids(
    db: &PgPool,
    seeds: &[(i64, f64)],
    viewer: Option<Uuid>,
    hidden: &[Uuid],
    limit: i64,
) -> sqlx::Result<Vec<Uuid>> {
    let (numbers, weights): (Vec<i64>, Vec<f64>) = seeds.iter().copied().unzip();
    let mut tx = db.begin().await?;
    sqlx::query("SELECT set_config('statement_timeout', $1, true)").bind(RANKING_TIME).execute(&mut *tx).await?;
    let ranked = sqlx::query_scalar::<_, Uuid>(RANKED)
        .bind(numbers)
        .bind(weights)
        .bind(hidden.to_vec())
        .bind(viewer)
        .bind(SOURCES)
        .bind(limit)
        .fetch_all(&mut *tx)
        .await;
    // Read only: the transaction ends as it is dropped.
    match ranked {
        // Cancelled for running too long (query_canceled).
        Err(sqlx::Error::Database(error)) if error.code().as_deref() == Some("57014") => Ok(Vec::new()),
        ranked => ranked,
    }
}

/// recommended_ids's query: $1 the seeds' numbers and $2 their weights, $3 the accounts left out,
/// $4 the viewer (or NULL), $5 how many seeds lead (SOURCES), $6 how many it ranks at most.
const RANKED: &str = "
    WITH seeds AS MATERIALIZED (
        SELECT l.id, l.category_id, l.price_normalized, l.price_normalized::float8 AS price, s.weight, s.n,
               CASE WHEN l.latitude IS NOT NULL THEN ll_to_earth(l.latitude::float8, l.longitude::float8) END AS place
        FROM unnest($1::bigint[], $2::float8[]) WITH ORDINALITY AS s (public_no, weight, n)
        JOIN listings l ON l.public_no = s.public_no
        WHERE l.status IN ('ACTIVE', 'RESERVED', 'SOLD')
    ),
    sources AS MATERIALIZED (
        SELECT * FROM seeds ORDER BY weight DESC, n LIMIT $5
    ),
    choices AS MATERIALIZED (
        SELECT s.category_id, s.weight, v.attribute_id, v.value_option_id AS option_id,
               o.parent_option_id IS NOT NULL AS narrow
        FROM sources s
        JOIN listing_attribute_values v ON v.listing_id = s.id
        JOIN attribute_options o ON o.id = v.value_option_id
    ),
    candidates AS (
        SELECT c.id FROM sources s CROSS JOIN LATERAL (
            SELECT l.id FROM listings l
            WHERE l.status = 'ACTIVE' AND l.category_id = s.category_id
              AND coalesce(l.price_normalized, 'Infinity') <= coalesce(s.price_normalized, 'Infinity')
            ORDER BY coalesce(l.price_normalized, 'Infinity') DESC, l.id DESC
            LIMIT 20) c
        UNION
        SELECT c.id FROM sources s CROSS JOIN LATERAL (
            SELECT l.id FROM listings l
            WHERE l.status = 'ACTIVE' AND l.category_id = s.category_id
              AND coalesce(l.price_normalized, 'Infinity') >= coalesce(s.price_normalized, 'Infinity')
            ORDER BY coalesce(l.price_normalized, 'Infinity'), l.id
            LIMIT 20) c
        UNION
        SELECT c.id FROM sources s CROSS JOIN LATERAL (
            SELECT l.id FROM listings l
            WHERE s.place IS NOT NULL AND l.status = 'ACTIVE' AND l.category_id = s.category_id
              AND l.latitude IS NOT NULL
            ORDER BY ll_to_earth(l.latitude::float8, l.longitude::float8) <-> s.place
            LIMIT 20) c
        UNION
        SELECT c.id FROM (SELECT DISTINCT category_id FROM sources) s CROSS JOIN LATERAL (
            SELECT l.id FROM listings l
            WHERE l.status = 'ACTIVE' AND l.category_id = s.category_id
            ORDER BY l.sort_at DESC, l.id DESC
            LIMIT 20) c
        UNION
        -- The newest 20 with a narrow option (a car's model), as related_listings reads them: off
        -- the option's index, newest first, each looked up by its id until 20 are up in the
        -- category; 400 at most, for an option whose listings are mostly gone or elsewhere.
        SELECT c.listing_id
        FROM (SELECT DISTINCT category_id, attribute_id, option_id FROM choices WHERE narrow) ch
        CROSS JOIN LATERAL (
            SELECT w.listing_id
            FROM (SELECT v.listing_id FROM listing_attribute_values v
                  WHERE v.attribute_id = ch.attribute_id AND v.value_option_id = ch.option_id
                  ORDER BY v.listing_id DESC
                  LIMIT 400) w
            WHERE (SELECT l.status = 'ACTIVE' AND l.category_id = ch.category_id
                   FROM listings l WHERE l.id = w.listing_id)
            LIMIT 20) c
    ),
    -- Each candidate's price and how new it is (one today, none two months on) as float8, whose
    -- sums are quicker than numeric's.
    pool AS MATERIALIZED (
        SELECT l.id, l.category_id, coalesce(l.seller_business_id, l.seller_user_id) AS seller,
               l.price_normalized::float8 AS price,
               CASE WHEN l.latitude IS NOT NULL THEN ll_to_earth(l.latitude::float8, l.longitude::float8) END AS place,
               greatest(0, 1 - extract(epoch FROM now() - l.sort_at)::float8 / 5184000) AS fresh
        FROM candidates c
        JOIN listings l ON l.id = c.id
        WHERE l.status = 'ACTIVE' AND l.id NOT IN (SELECT id FROM seeds)
          AND l.created_by_user_id <> ALL($3::uuid[])
          AND ($4::uuid IS NULL OR NOT manages_listing(l.seller_business_id, l.created_by_user_id, $4::uuid))
    ),
    -- Each candidate's choices, read once: joined to the seeds' at once, the planner read them
    -- again for each choice of their category (69 192 times for twelve cars, 112 ms).
    options AS MATERIALIZED (
        SELECT p.id, p.category_id, v.value_option_id AS option_id
        FROM pool p
        JOIN listing_attribute_values v ON v.listing_id = p.id
        WHERE v.value_option_id IS NOT NULL
    ),
    -- What each of the seeds' choices is worth to a candidate of their category that has it: the
    -- weights of the seeds that have it, a narrow one's three times, any other's half.
    worth AS (
        SELECT category_id, option_id, sum(weight * CASE WHEN narrow THEN 3 ELSE 0.5 END) AS worth
        FROM choices
        GROUP BY category_id, option_id
    ),
    -- The choices each shares with the seeds of its category.
    shared AS (
        SELECT o.id, sum(w.worth) AS score
        FROM options o
        JOIN worth w ON w.option_id = o.option_id AND w.category_id = o.category_id
        GROUP BY o.id
    ),
    -- Its price, its place and how new it is, for each seed of its category, weighed.
    alike AS (
        SELECT p.id, p.category_id, p.seller,
               sum(s.weight * (
                   3 * CASE WHEN p.price > 0 AND s.price > 0
                            THEN greatest(0, 1 - abs(ln(p.price / s.price)) / ln(2))
                            ELSE 0 END
                   + 2 * CASE WHEN p.place IS NOT NULL AND s.place IS NOT NULL
                              THEN greatest(0, 1 - (p.place <-> s.place) / 50000)
                              ELSE 0 END
                   + p.fresh)) AS score
        FROM pool p
        JOIN sources s ON s.category_id = p.category_id
        GROUP BY p.id, p.category_id, p.seller
    ),
    scored AS (
        SELECT a.id, a.category_id, a.score + coalesce(sh.score, 0) AS score,
               row_number() OVER (PARTITION BY a.seller ORDER BY a.score + coalesce(sh.score, 0) DESC, a.id)
                   AS seller_rank
        FROM alike a
        LEFT JOIN shared sh ON sh.id = a.id
    ),
    -- What each category's seeds weigh among all of them.
    shares AS (
        SELECT category_id, sum(weight) / (SELECT sum(weight) FROM sources) AS share
        FROM sources
        GROUP BY category_id
    ),
    spread AS (
        SELECT s.id, s.score, w.share,
               row_number() OVER (PARTITION BY s.category_id ORDER BY s.score DESC, s.id) AS category_rank
        FROM scored s
        JOIN shares w ON w.category_id = s.category_id
        WHERE s.seller_rank <= 2
    )
    SELECT id FROM spread
    ORDER BY (category_rank - 1) / share, score DESC, id
    LIMIT $6";

/// The listings a signed-in user showed an interest in, still on the site: the seeds of their
/// recommendations, with those they saw lately (recommended.rs). The numbers of the latest
/// KEPT_SEEDS they keep, and of the latest WRITTEN_SEEDS they wrote to the seller about.
pub async fn seed_listings(db: &PgPool, user_id: Uuid) -> sqlx::Result<(Vec<i64>, Vec<i64>)> {
    tokio::try_join!(
        sqlx::query_scalar(
            "SELECT l.public_no FROM saved_listings s JOIN listings l ON l.id = s.listing_id
             WHERE s.user_id = $1 AND l.status IN ('ACTIVE', 'RESERVED', 'SOLD')
             ORDER BY s.saved_at DESC LIMIT $2",
        )
        .bind(user_id)
        .bind(KEPT_SEEDS)
        .fetch_all(db),
        sqlx::query_scalar(
            "SELECT l.public_no FROM conversations c JOIN listings l ON l.id = c.listing_id
             WHERE c.buyer_user_id = $1 AND l.status IN ('ACTIVE', 'RESERVED', 'SOLD')
             ORDER BY coalesce(c.last_message_at, c.created_at) DESC LIMIT $2",
        )
        .bind(user_id)
        .bind(WRITTEN_SEEDS)
        .fetch_all(db),
    )
}
