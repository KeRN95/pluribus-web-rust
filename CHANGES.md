# Changes from the first version

Both versions were run on PostgreSQL 15.19. The problems below reproduce on the original schema, and `tests/schema_checks.sql` (70 checks) passes on this one.

## Data integrity

- **Attribute values weren't tied to their attribute.** The original accepted `vehicle_fuel = MANUAL` (an option of another attribute), text in a NUMBER attribute, and two values for a single-select attribute. Now composite foreign keys on `(attribute_id, option_id)` and `(attribute_id, data_type)` plus a CHECK on the value column prevent all three. Single-select values moved to `listing_attribute_values.value_option_id`, and `listing_attribute_options` is for MULTISELECT only.
- **Duplicate and open conversations.** A double click could create two threads for the same buyer and listing, and anyone could post into any conversation. Conversations now record `buyer_user_id` and the seller (`seller_user_id` or `seller_business_id`) with `UNIQUE (listing_id, buyer_user_id)`, and a message's sender must be a participant. SYSTEM messages have no sender.
- **Business listings had no person behind them.** With `seller_business_id` set there was no user to route buyer messages to or to audit. Added `listings.created_by_user_id`; a conversation's `seller_business_id` gives the business a shared inbox.
- **Ownership was stored twice.** `businesses.owner_user_id` and an `OWNER` row in `business_members` could name different people. Removed `owner_user_id`; a partial unique index allows one OWNER per business.
- **Reviews** accepted self-reviews and any number of reviews per reviewer and listing. Added a CHECK and `UNIQUE (listing_id, reviewer_user_id)`, plus an optional `conversation_id` as proof of contact.
- **Top-level locations could share a slug**, because `UNIQUE (parent_id, slug)` treats NULL parents as distinct. Now `UNIQUE NULLS NOT DISTINCT`.
- **A business could hold several ACTIVE subscriptions.** A partial unique index now allows one live subscription (TRIAL, ACTIVE, PAST_DUE); a new `PENDING` status covers renewals and unpaid sign-ups.
- **Inconsistent listings were accepted**: FREE with a price, FIXED without one, ACTIVE without `published_at`, SOLD without `sold_at`. CHECK constraints now refuse them (drafts may still lack a price).
- **Verifications**: `UNIQUE (user_id, verification_type)` allowed one row per type ever, and a verified phone stayed verified after the user changed the number. All attempts are now kept, `verified_value` records what was verified, and a partial unique index allows one VERIFIED row per type.
- **Automatic moderation couldn't be recorded** (`moderator_user_id NOT NULL`), although the README's flow relies on automatic approval. NULL now means "automated rule", and `report_id` links an action to its report.
- **Unvalidated formats.** Added `e164_phone` and `currency_code` domains, and range and both-or-neither checks for coordinates.
- **Staff role in `users.account_type`** (`ADMIN`). Removed; staff roles come from the identity provider.

## Money

- **Webhook retries weren't idempotent.** `provider_order_id` wasn't unique, and nothing de-duplicated webhook deliveries. Added `UNIQUE (provider, provider_order_id)`, a unique provider reference per transaction type, and `payment_webhook_events`.
- **Cascades deleted financial records.** Deleting a payment order removed its transactions, and deleting a business removed its subscription history. Financial tables no longer cascade.
- **`payment_orders.purpose_reference` was a foreign key without a constraint**, and subscriptions were never linked to payments. Added `payment_order_items` with real foreign keys (promotion product, listing, subscription) and the price paid. `listing_promotions.payment_order_item_id` links a promotion to its payment.

## Deletion policy

- **Mixed CASCADE and RESTRICT on `users`.** Hard-deleting a user silently erased reports and reviews about them, which are moderation evidence, while other foreign keys blocked the same delete. Now one rule applies (see README): personal-only data cascades, and everything other people rely on blocks the delete. A CHECK makes sure a DELETED user no longer holds an e-mail address or phone number.

## Search and ranking

- **The README's ordering kept any once-bumped listing above every newer one, permanently**: a listing bumped 30 days ago outranked one published an hour ago. Now `sort_at = greatest(published_at, bumped_at)` is a generated column with partial indexes, featured listings get a separate slot, and pagination is keyset instead of OFFSET.
- **Accent-sensitive search**: "shtepi" didn't match "Shtëpi". Added the `sq_unaccent` text search configuration, and trigram indexes on unaccented titles and location names.
- **`category_attributes.searchable` had no effect**, and title and description counted equally. Added `listings.search_extra` and a weighted search vector (title, then attributes, then description).
- **A price filter or sort compared lek and euro amounts directly.** Added `price_normalized` (EUR).
- **`listings.slug` was globally unique**, which forces "golf-7-2", "golf-7-3"… and breaks old URLs on title edits. Now not unique; route by `public_no`.

## Performance

- **Missing indexes** for hot lookups and foreign-key actions, including: `device_tokens.user_id` (every push notification), `payment_transactions.payment_order_id`, `conversations.listing_id`, all of a user's notifications (not only unread), `business_members.user_id`, `user_blocks.blocked_user_id`, `category_attributes.attribute_id`, `reviews.reviewer_user_id`, report and moderation-action columns, `business_subscriptions.plan_id`.
- **Redundant or unused indexes removed**: `idx_locations_parent` and `idx_attribute_options_attribute` duplicated unique constraints, and nothing queried the GIN index on `saved_searches.filters`. It was replaced by the indexes the alert jobs need.
- **Hot listing indexes are partial** on `status = 'ACTIVE'`.
- **UUID v7 primary keys** (`uuid_v7()`) instead of random v4, so inserts append to indexes instead of scattering across them.
- **`conversation_participants.last_read_message_id`** needed a circular foreign key with no index, and random UUIDs can't be ordered. Replaced by `last_read_at`, and `conversations.last_message_at` orders the inbox.

## Features the original implied but couldn't support

- `notification_preferences.price_drop_alerts` existed with no price history. Added `listing_price_history`, filled by trigger.
- Status history depended on every code path remembering to write it. It is now filled by trigger, with an optional actor and reason passed via `set_config`.
- Chat messages couldn't be reported. Added `reports.message_id` (the sender goes in `reported_user_id`) and one open report per person per listing.
- `vehicle_make` was a required SELECT with no options, so no vehicle listing could be created, and the model was free text, so it couldn't be filtered. Added dependent options (`parent_attribute_id`, `parent_option_id`), sample makes and models, and a MULTISELECT example (`vehicle_features`).
- Vehicle attributes were required on the Vehicles root (now Automjete), so also for motorcycles and spare parts. They now hang off a Makina (Cars) subcategory; inheritance down the tree and `categories.allow_listings` are documented.
- `listing_media` gained a processing status, MIME type, size and `sha256`.
- `outbox_events` for feeding OpenSearch, notifications and saved-search alerts reliably.

## Albania first

- Labels were English-only on a site whose default language is Albanian (`preferred_language` defaults to `sq`). Labels and category slugs are now Albanian (`Automjete`, `/automjete`; the country row is `Shqipëri`), with English in `name_i18n` / `label_i18n` / `description_i18n`.
- Seeded Albania's 12 counties and 61 municipalities.
- Listing prices default to lekë (`ALL`) instead of euros, and `categories.default_currency` preselects euros for cars and property.

## Category properties (migration 0003)

- Only Makina had properties. Every category now has some (see README, "Category properties"), including the electronics type that tells laptops from desktop PCs, and cars gained body type and colour.
- Inheriting properties down the category tree took a recursive query in every caller. The `effective_category_attributes` view does it once.
- A rent and a sale price sat in the same price sort with nothing to tell them apart. `categories.listing_types` says which kinds of listing a category takes, so property can be filtered by sale or rent; `categories.uses_condition` hides the new/used filter where it means nothing.
- `price_normalized` depended on a rate the application had to know. The `exchange_rates` table and `price_in_eur()` now convert both listing prices and price filter bounds.
- `search_extra` had to be assembled by hand. `listing_search_extra(listing)` builds it from the searchable properties, and `format_attribute_number()` formats numbers for display.

## Map search (migration 0005)

- Listings and locations had coordinates, but nothing could search by them. The `cube` and `earthdistance` extensions and a partial GiST index (`idx_listings_active_earth`) find the listings within a radius, nearest first, without PostGIS.
- Municipalities had no coordinates. Each now has its seat's point (from Wikidata), and a schema check keeps them all inside Albania.

## Operational

- Split into two sqlx migrations (`0001` schema, `0002` reference data) without `BEGIN`/`COMMIT`, since sqlx runs each migration in its own transaction. The reference data can be re-run safely.
- Tables live in the default `public` schema instead of a separate `marketplace` schema. With the separate schema, sqlx 0.8 recorded applied migrations in one schema and later looked for them in the other, and every client needed a custom search path.
- `pgcrypto` dropped (`gen_random_uuid()` is built in since PostgreSQL 13). Extensions are installed explicitly into `public`, and trigger functions schema-qualify their tables so they work under any search path.
- History tables default to `clock_timestamp()`, so several changes in one transaction keep their order.
- `audit_log.id` uses an identity column instead of `bigserial`.
