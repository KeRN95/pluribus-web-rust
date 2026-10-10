-- What a seller's card and profile say of how they answer and when they were last on the site
-- (web/listings.rs, browse.rs).

-- How quickly each seller answers: over the conversations of the last 90 days that a buyer wrote
-- in, by the seller (a store, or a private seller), how many there were, how many the seller's
-- side answered, and the middle time from the buyer's first message to that side's first answer
-- after it. Rebuilt with the other figures every 15 minutes (main.rs); a page reads one row.
CREATE MATERIALIZED VIEW seller_response_times AS
SELECT coalesce(c.seller_business_id, c.seller_user_id) AS seller_id,
       count(*) AS conversations,
       count(answer.at) AS answered,
       percentile_cont(0.5) WITHIN GROUP (ORDER BY extract(epoch FROM answer.at - asked.at)) AS median_seconds
FROM conversations c
CROSS JOIN LATERAL (SELECT min(m.created_at) AS at FROM messages m
                    WHERE m.conversation_id = c.id AND m.sender_user_id = c.buyer_user_id) asked
LEFT JOIN LATERAL (SELECT min(m.created_at) AS at FROM messages m
                   WHERE m.conversation_id = c.id AND m.sender_user_id <> c.buyer_user_id
                     AND m.created_at > asked.at) answer ON true
WHERE c.created_at > now() - interval '90 days' AND asked.at IS NOT NULL
GROUP BY 1;

-- One row per seller, which the rebuild needs to swap the figures in while pages read them.
CREATE UNIQUE INDEX ux_seller_response_times ON seller_response_times (seller_id);

-- users.last_seen_at, which the session lookup touches every ten minutes at most (db/sign_in.rs),
-- leaves updated_at as it was: that says when the account itself last changed.
CREATE FUNCTION set_users_updated_at() RETURNS trigger
    LANGUAGE plpgsql
AS $$
BEGIN
    IF to_jsonb(NEW) - 'last_seen_at' - 'updated_at' IS DISTINCT FROM to_jsonb(OLD) - 'last_seen_at' - 'updated_at' THEN
        NEW.updated_at := now();
    END IF;
    RETURN NEW;
END
$$;

DROP TRIGGER trg_users_updated_at ON users;
CREATE TRIGGER trg_users_updated_at
BEFORE UPDATE ON users FOR EACH ROW EXECUTE FUNCTION set_users_updated_at();
