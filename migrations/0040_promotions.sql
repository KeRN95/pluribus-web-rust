-- Promotions, which moderators give until there are payments (db/promotions.rs, /moderim): what
-- each does is copied onto the listing for the pages that read it on every card, as
-- featured_until already is. A highlighted listing's card stands out wherever it shows, until
-- highlighted_until; the product that does it, a week, with a placeholder price as the others'.
ALTER TABLE listings ADD COLUMN highlighted_until timestamptz;

INSERT INTO promotion_products (code, name, name_i18n, promotion_type, duration_hours, price_amount, currency)
VALUES ('HIGHLIGHT_7D', 'I theksuar për 7 ditë', '{"en": "Highlighted for 7 days"}', 'HIGHLIGHT', 168, 2.99, 'EUR')
ON CONFLICT DO NOTHING;

-- A moderator's giving or ending one is kept with the rest of what they do.
ALTER TABLE moderation_actions DROP CONSTRAINT moderation_actions_action_type_check;
ALTER TABLE moderation_actions ADD CONSTRAINT moderation_actions_action_type_check
    CHECK (action_type IN ('APPROVE_LISTING', 'REJECT_LISTING', 'SUSPEND_LISTING', 'RESTORE_LISTING', 'WARN_USER',
                           'SUSPEND_USER', 'RESTORE_USER', 'SUSPEND_BUSINESS', 'RESTORE_BUSINESS',
                           'GRANT_PROMOTION', 'END_PROMOTION'));
