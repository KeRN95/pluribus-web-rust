-- Marketplace database schema
-- Target: PostgreSQL 15+
-- Designed for a MerrJep / FINN-style classifieds marketplace.
-- Auth credentials are intentionally NOT stored here; integrate with Keycloak/Auth0/Cognito/etc.
--
-- sqlx runs each migration in its own transaction, so this file has no BEGIN/COMMIT.
-- Without sqlx:
--   psql -v ON_ERROR_STOP=1 --single-transaction -f migrations/0001_marketplace_schema.sql -f migrations/0002_marketplace_reference_data.sql

-- Everything lives in the public schema, next to these extensions, so clients need no
-- search_path setup.
CREATE EXTENSION IF NOT EXISTS citext   WITH SCHEMA public;
CREATE EXTENSION IF NOT EXISTS pg_trgm  WITH SCHEMA public;
CREATE EXTENSION IF NOT EXISTS unaccent WITH SCHEMA public;

-- ============================================================
-- COMMON FUNCTIONS / DOMAINS / TEXT SEARCH
-- ============================================================

-- Time-ordered UUIDs (RFC 9562 version 7). Random v4 keys land all over the B-tree; v7 keys
-- append at its right edge, which keeps hot tables (listings, messages, notifications) compact.
-- On PostgreSQL 18+ the built-in uuidv7() can replace this function.
CREATE OR REPLACE FUNCTION uuid_v7()
RETURNS uuid
LANGUAGE sql
VOLATILE
RETURN encode(
    set_bit(
        set_bit(
            overlay(uuid_send(gen_random_uuid())
                    PLACING substring(int8send(floor(extract(epoch FROM clock_timestamp()) * 1000)::bigint) FROM 3)
                    FROM 1 FOR 6),
            52, 1),
        53, 1),
    'hex')::uuid;

-- unaccent() is only STABLE (it finds its dictionary through search_path), so it cannot be used
-- in an index expression. This wrapper pins the dictionary and can be.
CREATE OR REPLACE FUNCTION immutable_unaccent(text)
RETURNS text
LANGUAGE sql
IMMUTABLE PARALLEL SAFE STRICT
RETURN public.unaccent('public.unaccent'::regdictionary, $1);

CREATE OR REPLACE FUNCTION set_updated_at()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    NEW.updated_at = now();
    RETURN NEW;
END;
$$;

CREATE DOMAIN e164_phone AS varchar(16)
    CHECK (VALUE ~ '^\+[1-9][0-9]{6,14}$');

CREATE DOMAIN currency_code AS char(3)
    CHECK (VALUE ~ '^[A-Z]{3}$');

-- Albanian users routinely type "e"/"c" for "ë"/"ç" (shtepi, Tirane, Korce). PostgreSQL has no
-- Albanian stemmer, so this is the 'simple' configuration plus accent folding. Use it for
-- queries too: websearch_to_tsquery('sq_unaccent', :q).
CREATE TEXT SEARCH CONFIGURATION sq_unaccent (COPY = pg_catalog.simple);
ALTER TEXT SEARCH CONFIGURATION sq_unaccent
    ALTER MAPPING FOR word, hword, hword_part, numword, numhword, hword_numpart
    WITH public.unaccent, pg_catalog.simple;

-- ============================================================
-- USERS / IDENTITY / BUSINESS ACCOUNTS
-- ============================================================

CREATE TABLE users (
    id                  uuid PRIMARY KEY DEFAULT uuid_v7(),
    auth_subject        varchar(255) NOT NULL UNIQUE,
    email               citext UNIQUE,
    phone_e164          e164_phone UNIQUE,
    display_name        varchar(120) NOT NULL,
    avatar_url          text,
    -- Staff roles (moderator, admin) come from the identity provider, not from this column.
    account_type        varchar(20) NOT NULL DEFAULT 'PRIVATE'
                        CHECK (account_type IN ('PRIVATE', 'BUSINESS')),
    status              varchar(20) NOT NULL DEFAULT 'ACTIVE'
                        CHECK (status IN ('PENDING', 'ACTIVE', 'SUSPENDED', 'DELETED')),
    preferred_language  varchar(10) NOT NULL DEFAULT 'sq',
    -- Write at most every few minutes; updating it on every request churns this row.
    last_seen_at        timestamptz,
    deleted_at          timestamptz,
    created_at          timestamptz NOT NULL DEFAULT now(),
    updated_at          timestamptz NOT NULL DEFAULT now(),
    -- Users with activity are never hard-deleted: erasure means anonymising (see README).
    CONSTRAINT chk_users_deleted_anonymised
        CHECK (status <> 'DELETED' OR (deleted_at IS NOT NULL AND email IS NULL AND phone_e164 IS NULL))
);

CREATE TABLE user_verifications (
    id                  uuid PRIMARY KEY DEFAULT uuid_v7(),
    user_id             uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    verification_type   varchar(30) NOT NULL
                        CHECK (verification_type IN ('EMAIL', 'PHONE', 'IDENTITY', 'BUSINESS')),
    -- The e-mail address or phone number that was verified. If the user changes it later, this
    -- row no longer matches users.email / users.phone_e164 and the new value counts as unverified.
    verified_value      varchar(255),
    status              varchar(20) NOT NULL DEFAULT 'PENDING'
                        CHECK (status IN ('PENDING', 'VERIFIED', 'REJECTED', 'EXPIRED', 'REVOKED')),
    provider            varchar(80),
    provider_reference  varchar(255),
    verified_at         timestamptz,
    expires_at          timestamptz,
    created_at          timestamptz NOT NULL DEFAULT now(),
    updated_at          timestamptz NOT NULL DEFAULT now(),
    CHECK (verification_type NOT IN ('EMAIL', 'PHONE') OR verified_value IS NOT NULL)
);

CREATE INDEX idx_user_verifications_user ON user_verifications(user_id, verification_type, created_at DESC);
-- Every attempt is kept; at most one verification per type is current.
CREATE UNIQUE INDEX ux_user_verifications_current
    ON user_verifications(user_id, verification_type)
    WHERE status = 'VERIFIED';

CREATE TABLE businesses (
    id                  uuid PRIMARY KEY DEFAULT uuid_v7(),
    legal_name          varchar(200) NOT NULL,
    display_name        varchar(160) NOT NULL,
    slug                varchar(180) NOT NULL UNIQUE,
    registration_no     varchar(100),
    tax_no              varchar(100),
    description         text,
    logo_url            text,
    cover_url           text,
    website_url         text,
    public_phone        e164_phone,
    public_email        citext,
    status              varchar(20) NOT NULL DEFAULT 'ACTIVE'
                        CHECK (status IN ('PENDING', 'ACTIVE', 'SUSPENDED', 'DELETED')),
    created_at          timestamptz NOT NULL DEFAULT now(),
    updated_at          timestamptz NOT NULL DEFAULT now()
);

-- The single source of truth for who owns and manages a business.
CREATE TABLE business_members (
    business_id         uuid NOT NULL REFERENCES businesses(id) ON DELETE CASCADE,
    user_id             uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role                varchar(30) NOT NULL
                        CHECK (role IN ('OWNER', 'ADMIN', 'EDITOR', 'VIEWER')),
    created_at          timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (business_id, user_id)
);

CREATE UNIQUE INDEX ux_business_members_one_owner ON business_members(business_id) WHERE role = 'OWNER';
CREATE INDEX idx_business_members_user ON business_members(user_id);

CREATE TABLE user_blocks (
    blocker_user_id     uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    blocked_user_id     uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at          timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (blocker_user_id, blocked_user_id),
    CHECK (blocker_user_id <> blocked_user_id)
);

CREATE INDEX idx_user_blocks_blocked ON user_blocks(blocked_user_id);

-- ============================================================
-- LOCATION
-- ============================================================

CREATE TABLE locations (
    id                  uuid PRIMARY KEY DEFAULT uuid_v7(),
    parent_id           uuid REFERENCES locations(id) ON DELETE RESTRICT,
    location_type       varchar(20) NOT NULL
                        CHECK (location_type IN ('COUNTRY', 'REGION', 'MUNICIPALITY', 'CITY', 'AREA')),
    name                varchar(160) NOT NULL,
    slug                varchar(180) NOT NULL,
    country_code        char(2) NOT NULL DEFAULT 'AL',
    latitude            numeric(9,6) CHECK (latitude BETWEEN -90 AND 90),
    longitude           numeric(9,6) CHECK (longitude BETWEEN -180 AND 180),
    sort_order          integer NOT NULL DEFAULT 0,
    active              boolean NOT NULL DEFAULT true,
    CHECK ((latitude IS NULL) = (longitude IS NULL)),
    CHECK (parent_id <> id),
    -- NULLS NOT DISTINCT so two top-level rows (parent_id NULL) cannot share a slug either.
    UNIQUE NULLS NOT DISTINCT (parent_id, slug)
);

-- The (parent_id, slug) unique index also serves lookups by parent.
-- Accent-insensitive autocomplete: "korce" finds "Korçë".
CREATE INDEX idx_locations_name_trgm ON locations USING gin (immutable_unaccent(name) gin_trgm_ops);

-- ============================================================
-- CATEGORIES + DYNAMIC ATTRIBUTES
-- ============================================================

CREATE TABLE categories (
    id                  uuid PRIMARY KEY DEFAULT uuid_v7(),
    parent_id           uuid REFERENCES categories(id) ON DELETE RESTRICT,
    -- Labels are in Albanian, the site's language. The *_i18n columns hold other languages keyed
    -- by language code, e.g. {"en": "Vehicles"}; read coalesce(name_i18n ->> :lang, name).
    name                varchar(160) NOT NULL,
    name_i18n           jsonb NOT NULL DEFAULT '{}'::jsonb CHECK (jsonb_typeof(name_i18n) = 'object'),
    slug                varchar(180) NOT NULL UNIQUE,
    description         text,
    description_i18n    jsonb NOT NULL DEFAULT '{}'::jsonb CHECK (jsonb_typeof(description_i18n) = 'object'),
    icon_key            varchar(100),
    sort_order          integer NOT NULL DEFAULT 0,
    active              boolean NOT NULL DEFAULT true,
    allow_private       boolean NOT NULL DEFAULT true,
    allow_business      boolean NOT NULL DEFAULT true,
    -- False for categories that only group subcategories.
    allow_listings      boolean NOT NULL DEFAULT true,
    -- Currency preselected in the listing form: cars and property are priced in euros in
    -- Albania, most other things in lekë.
    default_currency    currency_code NOT NULL DEFAULT 'ALL',
    created_at          timestamptz NOT NULL DEFAULT now(),
    updated_at          timestamptz NOT NULL DEFAULT now(),
    CHECK (parent_id <> id)
);

CREATE INDEX idx_categories_parent ON categories(parent_id);

CREATE TABLE attributes (
    id                  uuid PRIMARY KEY DEFAULT uuid_v7(),
    code                varchar(100) NOT NULL UNIQUE,
    name                varchar(160) NOT NULL,
    name_i18n           jsonb NOT NULL DEFAULT '{}'::jsonb CHECK (jsonb_typeof(name_i18n) = 'object'),
    data_type           varchar(20) NOT NULL
                        CHECK (data_type IN ('TEXT', 'NUMBER', 'BOOLEAN', 'DATE', 'SELECT', 'MULTISELECT')),
    unit                varchar(30),
    -- Dependent dropdowns: each vehicle_model option belongs to a vehicle_make option.
    parent_attribute_id uuid REFERENCES attributes(id),
    validation_json     jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at          timestamptz NOT NULL DEFAULT now(),
    updated_at          timestamptz NOT NULL DEFAULT now(),
    CHECK (parent_attribute_id <> id),
    -- Target of the composite foreign keys that make every stored value match its attribute's type.
    UNIQUE (id, data_type)
);

CREATE TABLE attribute_options (
    id                  uuid PRIMARY KEY DEFAULT uuid_v7(),
    attribute_id        uuid NOT NULL REFERENCES attributes(id) ON DELETE CASCADE,
    parent_option_id    uuid REFERENCES attribute_options(id),
    value_code          varchar(100) NOT NULL,
    label               varchar(160) NOT NULL,
    label_i18n          jsonb NOT NULL DEFAULT '{}'::jsonb CHECK (jsonb_typeof(label_i18n) = 'object'),
    sort_order          integer NOT NULL DEFAULT 0,
    active              boolean NOT NULL DEFAULT true,
    UNIQUE (attribute_id, value_code),
    -- Referenced as (attribute_id, option_id), so an option can only be stored for its own attribute.
    UNIQUE (attribute_id, id)
);

-- The (attribute_id, value_code) unique index also serves lookups by attribute.
CREATE INDEX idx_attribute_options_parent ON attribute_options(parent_option_id) WHERE parent_option_id IS NOT NULL;

CREATE TABLE category_attributes (
    category_id         uuid NOT NULL REFERENCES categories(id) ON DELETE CASCADE,
    attribute_id        uuid NOT NULL REFERENCES attributes(id) ON DELETE CASCADE,
    required            boolean NOT NULL DEFAULT false,
    -- Searchable values are copied into listings.search_extra and so into full-text search.
    searchable          boolean NOT NULL DEFAULT true,
    filterable          boolean NOT NULL DEFAULT true,
    show_in_card        boolean NOT NULL DEFAULT false,
    show_in_details     boolean NOT NULL DEFAULT true,
    sort_order          integer NOT NULL DEFAULT 0,
    PRIMARY KEY (category_id, attribute_id)
);

CREATE INDEX idx_category_attributes_attribute ON category_attributes(attribute_id);

-- ============================================================
-- LISTINGS
-- ============================================================

CREATE TABLE listings (
    id                  uuid PRIMARY KEY DEFAULT uuid_v7(),
    public_no           bigint GENERATED ALWAYS AS IDENTITY UNIQUE,
    -- The seller is a private user or a business (exactly one)...
    seller_user_id      uuid REFERENCES users(id),
    seller_business_id  uuid REFERENCES businesses(id),
    -- ...but a person always created the listing, which business listings need for
    -- permissions, audit and routing buyer messages.
    created_by_user_id  uuid NOT NULL REFERENCES users(id),
    category_id         uuid NOT NULL REFERENCES categories(id),
    location_id         uuid REFERENCES locations(id),
    -- Not unique: route by public_no (/listing/12345-golf-7-tdi) so the slug can follow title edits.
    slug                varchar(220) NOT NULL,
    title               varchar(180) NOT NULL,
    description         text NOT NULL,
    listing_type        varchar(20) NOT NULL DEFAULT 'SELL'
                        CHECK (listing_type IN ('SELL', 'BUY', 'RENT', 'WANTED_RENT', 'SERVICE', 'JOB')),
    condition           varchar(20)
                        CHECK (condition IS NULL OR condition IN ('NEW', 'LIKE_NEW', 'USED', 'FOR_PARTS')),
    price_amount        numeric(14,2) CHECK (price_amount IS NULL OR price_amount >= 0),
    currency            currency_code NOT NULL DEFAULT 'ALL',
    price_type          varchar(20) NOT NULL DEFAULT 'FIXED'
                        CHECK (price_type IN ('FIXED', 'NEGOTIABLE', 'FROM', 'FREE', 'ON_REQUEST')),
    -- price_amount converted to EUR by the application, and refreshed when exchange rates move.
    -- Listings are priced in both ALL and EUR, so filter and sort on this column, not price_amount.
    price_normalized    numeric(14,2) CHECK (price_normalized IS NULL OR price_normalized >= 0),
    -- For private sellers store an approximate point (neighbourhood), not their front door.
    latitude            numeric(9,6) CHECK (latitude BETWEEN -90 AND 90),
    longitude           numeric(9,6) CHECK (longitude BETWEEN -180 AND 180),
    status              varchar(20) NOT NULL DEFAULT 'DRAFT'
                        CHECK (status IN ('DRAFT', 'PENDING_REVIEW', 'ACTIVE', 'RESERVED', 'SOLD', 'EXPIRED', 'REJECTED', 'SUSPENDED', 'DELETED')),
    rejection_reason    text,
    featured_until      timestamptz,
    bumped_at           timestamptz,
    published_at        timestamptz,
    expires_at          timestamptz,
    sold_at             timestamptz,
    -- "Newest first" ranking key: a bump lifts the listing to the top, then newer listings push it down.
    sort_at             timestamptz GENERATED ALWAYS AS (greatest(published_at, bumped_at)) STORED,
    -- Labels of the listing's searchable attributes ("Volkswagen Golf Naftë Automatike"),
    -- written by the application whenever the attribute values change.
    search_extra        text,
    created_at          timestamptz NOT NULL DEFAULT now(),
    updated_at          timestamptz NOT NULL DEFAULT now(),
    search_vector       tsvector GENERATED ALWAYS AS (
                            setweight(to_tsvector('public.sq_unaccent'::regconfig, coalesce(title, '')), 'A') ||
                            setweight(to_tsvector('public.sq_unaccent'::regconfig, coalesce(search_extra, '')), 'B') ||
                            setweight(to_tsvector('public.sq_unaccent'::regconfig, coalesce(description, '')), 'C')
                        ) STORED,
    CONSTRAINT chk_listings_seller
        CHECK (num_nonnulls(seller_user_id, seller_business_id) = 1),
    -- Drafts may be incomplete; anything past DRAFT needs a price that fits its price_type.
    CONSTRAINT chk_listings_price
        CHECK (status = 'DRAFT' OR CASE price_type
                                       WHEN 'FREE'       THEN coalesce(price_amount, 0) = 0
                                       WHEN 'ON_REQUEST' THEN price_amount IS NULL
                                       ELSE price_amount IS NOT NULL
                                   END),
    CONSTRAINT chk_listings_published
        CHECK (status NOT IN ('ACTIVE', 'RESERVED', 'SOLD', 'EXPIRED') OR published_at IS NOT NULL),
    CONSTRAINT chk_listings_sold
        CHECK (status <> 'SOLD' OR sold_at IS NOT NULL),
    CONSTRAINT chk_listings_coordinates
        CHECK ((latitude IS NULL) = (longitude IS NULL))
);

-- Browsing and search only read ACTIVE listings, so the hot indexes are partial.
-- (sort_at, id) supports keyset pagination; see README.
CREATE INDEX idx_listings_active_sort          ON listings(sort_at DESC, id DESC)              WHERE status = 'ACTIVE';
CREATE INDEX idx_listings_active_category_sort ON listings(category_id, sort_at DESC, id DESC) WHERE status = 'ACTIVE';
CREATE INDEX idx_listings_active_location_sort ON listings(location_id, sort_at DESC, id DESC) WHERE status = 'ACTIVE';
CREATE INDEX idx_listings_active_price         ON listings(category_id, price_normalized)       WHERE status = 'ACTIVE';
CREATE INDEX idx_listings_active_featured      ON listings(category_id, featured_until)         WHERE status = 'ACTIVE' AND featured_until IS NOT NULL;
CREATE INDEX idx_listings_search_vector        ON listings USING gin(search_vector)              WHERE status = 'ACTIVE';
CREATE INDEX idx_listings_title_trgm           ON listings USING gin(immutable_unaccent(title) gin_trgm_ops) WHERE status = 'ACTIVE';
-- Seller dashboards, moderation queue and background jobs.
CREATE INDEX idx_listings_seller_user     ON listings(seller_user_id, status, created_at DESC);
CREATE INDEX idx_listings_seller_business ON listings(seller_business_id, status, created_at DESC);
CREATE INDEX idx_listings_created_by      ON listings(created_by_user_id);
CREATE INDEX idx_listings_pending_review  ON listings(created_at) WHERE status = 'PENDING_REVIEW';
CREATE INDEX idx_listings_expiry          ON listings(expires_at) WHERE status = 'ACTIVE';

CREATE TABLE listing_contacts (
    listing_id          uuid PRIMARY KEY REFERENCES listings(id) ON DELETE CASCADE,
    contact_name        varchar(120),
    phone_e164          e164_phone,
    email               citext,
    whatsapp_e164       e164_phone,
    website_url         text,
    show_phone          boolean NOT NULL DEFAULT true,
    show_email          boolean NOT NULL DEFAULT false,
    allow_messages      boolean NOT NULL DEFAULT true
);

CREATE TABLE listing_media (
    id                  uuid PRIMARY KEY DEFAULT uuid_v7(),
    listing_id          uuid NOT NULL REFERENCES listings(id) ON DELETE CASCADE,
    media_type          varchar(10) NOT NULL CHECK (media_type IN ('IMAGE', 'VIDEO')),
    -- Uploaded -> virus scan / image moderation -> READY. Only READY media is shown publicly.
    status              varchar(20) NOT NULL DEFAULT 'PROCESSING'
                        CHECK (status IN ('PROCESSING', 'READY', 'REJECTED')),
    storage_key         text NOT NULL UNIQUE,
    public_url          text,
    thumbnail_url       text,
    mime_type           varchar(100),
    size_bytes          bigint CHECK (size_bytes > 0),
    width_px            integer CHECK (width_px > 0),
    height_px           integer CHECK (height_px > 0),
    -- The same photo reused across unrelated listings is a classic scam signal.
    sha256              bytea CHECK (octet_length(sha256) = 32),
    sort_order          integer NOT NULL DEFAULT 0,
    is_cover            boolean NOT NULL DEFAULT false,
    created_at          timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX idx_listing_media_listing ON listing_media(listing_id, sort_order);
CREATE INDEX idx_listing_media_sha256 ON listing_media(sha256) WHERE sha256 IS NOT NULL;
CREATE UNIQUE INDEX ux_listing_media_one_cover
    ON listing_media(listing_id)
    WHERE is_cover = true;

-- TEXT / NUMBER / BOOLEAN / DATE / SELECT values, one row per (listing, attribute).
-- data_type repeats the attribute's type: the composite foreign key rejects a mismatch and the
-- CHECK then requires the matching value column. MULTISELECT goes to listing_attribute_options.
CREATE TABLE listing_attribute_values (
    listing_id          uuid NOT NULL REFERENCES listings(id) ON DELETE CASCADE,
    attribute_id        uuid NOT NULL,
    data_type           varchar(20) NOT NULL,
    value_text          text,
    value_number        numeric(20,6),
    value_boolean       boolean,
    value_date          date,
    value_option_id     uuid,
    created_at          timestamptz NOT NULL DEFAULT now(),
    updated_at          timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (listing_id, attribute_id),
    FOREIGN KEY (attribute_id, data_type) REFERENCES attributes(id, data_type) ON DELETE CASCADE,
    FOREIGN KEY (attribute_id, value_option_id) REFERENCES attribute_options(attribute_id, id),
    CHECK (num_nonnulls(value_text, value_number, value_boolean, value_date, value_option_id) = 1),
    CHECK (CASE data_type
               WHEN 'TEXT'    THEN value_text IS NOT NULL
               WHEN 'NUMBER'  THEN value_number IS NOT NULL
               WHEN 'BOOLEAN' THEN value_boolean IS NOT NULL
               WHEN 'DATE'    THEN value_date IS NOT NULL
               WHEN 'SELECT'  THEN value_option_id IS NOT NULL
               ELSE false
           END)
);

CREATE INDEX idx_lav_attribute_number  ON listing_attribute_values(attribute_id, value_number, listing_id)    WHERE value_number IS NOT NULL;
CREATE INDEX idx_lav_attribute_option  ON listing_attribute_values(attribute_id, value_option_id, listing_id) WHERE value_option_id IS NOT NULL;
CREATE INDEX idx_lav_attribute_boolean ON listing_attribute_values(attribute_id, value_boolean, listing_id)   WHERE value_boolean IS NOT NULL;
CREATE INDEX idx_lav_attribute_date    ON listing_attribute_values(attribute_id, value_date, listing_id)      WHERE value_date IS NOT NULL;
CREATE INDEX idx_lav_attribute_text_trgm ON listing_attribute_values USING gin(value_text gin_trgm_ops)      WHERE value_text IS NOT NULL;

-- MULTISELECT values, one row per chosen option.
CREATE TABLE listing_attribute_options (
    listing_id          uuid NOT NULL REFERENCES listings(id) ON DELETE CASCADE,
    attribute_id        uuid NOT NULL,
    -- Always 'MULTISELECT'; present only so the foreign key below can check the attribute's type.
    data_type           varchar(20) NOT NULL DEFAULT 'MULTISELECT' CHECK (data_type = 'MULTISELECT'),
    option_id           uuid NOT NULL,
    PRIMARY KEY (listing_id, attribute_id, option_id),
    FOREIGN KEY (attribute_id, data_type) REFERENCES attributes(id, data_type) ON DELETE CASCADE,
    FOREIGN KEY (attribute_id, option_id) REFERENCES attribute_options(attribute_id, id)
);

CREATE INDEX idx_lao_filter ON listing_attribute_options(attribute_id, option_id, listing_id);

-- Filled by trigger on every status change (see TRIGGERS), including bulk jobs.
-- clock_timestamp() keeps several changes made in one transaction in order.
CREATE TABLE listing_status_history (
    id                  uuid PRIMARY KEY DEFAULT uuid_v7(),
    listing_id          uuid NOT NULL REFERENCES listings(id) ON DELETE CASCADE,
    from_status         varchar(20),
    to_status           varchar(20) NOT NULL,
    reason              text,
    changed_by_user_id  uuid REFERENCES users(id),
    created_at          timestamptz NOT NULL DEFAULT clock_timestamp()
);

CREATE INDEX idx_listing_status_history_listing ON listing_status_history(listing_id, created_at DESC);
CREATE INDEX idx_listing_status_history_user ON listing_status_history(changed_by_user_id) WHERE changed_by_user_id IS NOT NULL;

-- Filled by trigger on every price change; feeds price-drop alerts and "price reduced" badges.
CREATE TABLE listing_price_history (
    id                  uuid PRIMARY KEY DEFAULT uuid_v7(),
    listing_id          uuid NOT NULL REFERENCES listings(id) ON DELETE CASCADE,
    price_amount        numeric(14,2),
    currency            currency_code NOT NULL,
    price_type          varchar(20) NOT NULL,
    changed_at          timestamptz NOT NULL DEFAULT clock_timestamp()
);

CREATE INDEX idx_listing_price_history_listing ON listing_price_history(listing_id, changed_at DESC);

-- ============================================================
-- FAVORITES / SAVED SEARCHES / ALERTS
-- ============================================================

CREATE TABLE favorites (
    user_id             uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    listing_id          uuid NOT NULL REFERENCES listings(id) ON DELETE CASCADE,
    created_at          timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, listing_id)
);

CREATE INDEX idx_favorites_listing ON favorites(listing_id);

CREATE TABLE saved_searches (
    id                  uuid PRIMARY KEY DEFAULT uuid_v7(),
    user_id             uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name                varchar(120) NOT NULL,
    category_id         uuid REFERENCES categories(id),
    query_text          varchar(300),
    filters             jsonb NOT NULL DEFAULT '{}'::jsonb,
    alerts_enabled      boolean NOT NULL DEFAULT true,
    alert_frequency     varchar(20) NOT NULL DEFAULT 'INSTANT'
                        CHECK (alert_frequency IN ('INSTANT', 'DAILY', 'WEEKLY')),
    last_alerted_at     timestamptz,
    created_at          timestamptz NOT NULL DEFAULT now(),
    updated_at          timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX idx_saved_searches_user ON saved_searches(user_id, created_at DESC);
-- Instant alerts: saved searches to test when a listing is published in a category.
CREATE INDEX idx_saved_searches_instant ON saved_searches(category_id)
    WHERE alerts_enabled AND alert_frequency = 'INSTANT';
-- Daily/weekly digests: which searches are due.
CREATE INDEX idx_saved_searches_digest_due ON saved_searches(alert_frequency, last_alerted_at)
    WHERE alerts_enabled AND alert_frequency <> 'INSTANT';

-- ============================================================
-- CHAT / MESSAGING
-- ============================================================

CREATE TABLE conversations (
    id                  uuid PRIMARY KEY DEFAULT uuid_v7(),
    listing_id          uuid REFERENCES listings(id) ON DELETE SET NULL,
    -- Both sides are copied from the listing when the thread starts, so it still makes sense
    -- after the listing is deleted. A business seller's inbox is shared by its members.
    buyer_user_id       uuid NOT NULL REFERENCES users(id),
    seller_user_id      uuid REFERENCES users(id),
    seller_business_id  uuid REFERENCES businesses(id),
    last_message_at     timestamptz,
    created_at          timestamptz NOT NULL DEFAULT now(),
    updated_at          timestamptz NOT NULL DEFAULT now(),
    CHECK (num_nonnulls(seller_user_id, seller_business_id) = 1),
    CHECK (buyer_user_id <> seller_user_id),
    -- One thread per buyer per listing: use INSERT ... ON CONFLICT (listing_id, buyer_user_id).
    UNIQUE (listing_id, buyer_user_id)
);

CREATE INDEX idx_conversations_buyer ON conversations(buyer_user_id, last_message_at DESC);
CREATE INDEX idx_conversations_seller_user ON conversations(seller_user_id, last_message_at DESC)
    WHERE seller_user_id IS NOT NULL;
CREATE INDEX idx_conversations_seller_business ON conversations(seller_business_id, last_message_at DESC)
    WHERE seller_business_id IS NOT NULL;

-- Per-person state. For a business seller, add each member who opens the thread.
CREATE TABLE conversation_participants (
    conversation_id     uuid NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    user_id             uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    joined_at           timestamptz NOT NULL DEFAULT now(),
    -- Unread = messages from others created after this moment.
    last_read_at        timestamptz,
    archived_at         timestamptz,
    muted_until         timestamptz,
    PRIMARY KEY (conversation_id, user_id)
);

CREATE INDEX idx_conversation_participants_user ON conversation_participants(user_id, conversation_id);

CREATE TABLE messages (
    id                  uuid PRIMARY KEY DEFAULT uuid_v7(),
    conversation_id     uuid NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    -- NULL only for SYSTEM messages ("this item was marked as sold").
    sender_user_id      uuid,
    message_type        varchar(20) NOT NULL DEFAULT 'TEXT'
                        CHECK (message_type IN ('TEXT', 'IMAGE', 'OFFER', 'SYSTEM')),
    body                text,
    metadata            jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at          timestamptz NOT NULL DEFAULT now(),
    edited_at           timestamptz,
    deleted_at          timestamptz,
    -- The sender must be a participant of this conversation.
    FOREIGN KEY (conversation_id, sender_user_id)
        REFERENCES conversation_participants(conversation_id, user_id),
    CHECK ((message_type = 'SYSTEM') = (sender_user_id IS NULL)),
    CHECK (message_type <> 'TEXT' OR body IS NOT NULL OR deleted_at IS NOT NULL)
);

CREATE INDEX idx_messages_conversation ON messages(conversation_id, created_at DESC);
CREATE INDEX idx_messages_sender ON messages(sender_user_id, created_at DESC);

-- ============================================================
-- REVIEWS / REPUTATION
-- ============================================================

CREATE TABLE reviews (
    id                  uuid PRIMARY KEY DEFAULT uuid_v7(),
    listing_id          uuid REFERENCES listings(id) ON DELETE SET NULL,
    -- The conversation that proves the two people actually dealt with each other.
    conversation_id     uuid REFERENCES conversations(id) ON DELETE SET NULL,
    reviewer_user_id    uuid NOT NULL REFERENCES users(id),
    reviewed_user_id    uuid REFERENCES users(id),
    reviewed_business_id uuid REFERENCES businesses(id),
    rating              smallint NOT NULL CHECK (rating BETWEEN 1 AND 5),
    comment             text,
    status              varchar(20) NOT NULL DEFAULT 'PUBLISHED'
                        CHECK (status IN ('PENDING', 'PUBLISHED', 'HIDDEN', 'DELETED')),
    created_at          timestamptz NOT NULL DEFAULT now(),
    updated_at          timestamptz NOT NULL DEFAULT now(),
    CHECK (num_nonnulls(reviewed_user_id, reviewed_business_id) = 1),
    CHECK (reviewer_user_id <> reviewed_user_id),
    UNIQUE (listing_id, reviewer_user_id)
);

CREATE INDEX idx_reviews_user ON reviews(reviewed_user_id, status, created_at DESC);
CREATE INDEX idx_reviews_business ON reviews(reviewed_business_id, status, created_at DESC);
CREATE INDEX idx_reviews_reviewer ON reviews(reviewer_user_id, created_at DESC);
CREATE INDEX idx_reviews_conversation ON reviews(conversation_id) WHERE conversation_id IS NOT NULL;

-- ============================================================
-- PROMOTIONS / BILLING / BUSINESS SUBSCRIPTIONS
-- ============================================================
-- Financial rows (orders, items, transactions, subscriptions) are never cascade-deleted.

CREATE TABLE promotion_products (
    id                  uuid PRIMARY KEY DEFAULT uuid_v7(),
    code                varchar(60) NOT NULL UNIQUE,
    name                varchar(120) NOT NULL,
    name_i18n           jsonb NOT NULL DEFAULT '{}'::jsonb CHECK (jsonb_typeof(name_i18n) = 'object'),
    promotion_type      varchar(30) NOT NULL
                        CHECK (promotion_type IN ('FEATURED', 'BUMP', 'HIGHLIGHT', 'HOMEPAGE', 'CATEGORY_TOP')),
    duration_hours      integer CHECK (duration_hours IS NULL OR duration_hours > 0),
    price_amount        numeric(12,2) NOT NULL CHECK (price_amount >= 0),
    currency            currency_code NOT NULL DEFAULT 'EUR',
    active              boolean NOT NULL DEFAULT true,
    created_at          timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE subscription_plans (
    id                  uuid PRIMARY KEY DEFAULT uuid_v7(),
    code                varchar(60) NOT NULL UNIQUE,
    name                varchar(120) NOT NULL,
    name_i18n           jsonb NOT NULL DEFAULT '{}'::jsonb CHECK (jsonb_typeof(name_i18n) = 'object'),
    price_amount        numeric(12,2) NOT NULL CHECK (price_amount >= 0),
    currency            currency_code NOT NULL DEFAULT 'EUR',
    billing_period      varchar(20) NOT NULL CHECK (billing_period IN ('MONTHLY', 'YEARLY')),
    listing_limit       integer CHECK (listing_limit IS NULL OR listing_limit >= 0),
    included_promotions integer NOT NULL DEFAULT 0 CHECK (included_promotions >= 0),
    features            jsonb NOT NULL DEFAULT '{}'::jsonb,
    active              boolean NOT NULL DEFAULT true
);

CREATE TABLE business_subscriptions (
    id                  uuid PRIMARY KEY DEFAULT uuid_v7(),
    business_id         uuid NOT NULL REFERENCES businesses(id),
    plan_id             uuid NOT NULL REFERENCES subscription_plans(id),
    -- PENDING until the first payment succeeds.
    status              varchar(20) NOT NULL DEFAULT 'PENDING'
                        CHECK (status IN ('PENDING', 'TRIAL', 'ACTIVE', 'PAST_DUE', 'CANCELLED', 'EXPIRED')),
    starts_at           timestamptz NOT NULL,
    ends_at             timestamptz,
    auto_renew          boolean NOT NULL DEFAULT true,
    created_at          timestamptz NOT NULL DEFAULT now(),
    updated_at          timestamptz NOT NULL DEFAULT now(),
    CHECK (ends_at IS NULL OR ends_at > starts_at)
);

CREATE INDEX idx_business_subscriptions_business ON business_subscriptions(business_id, status);
CREATE INDEX idx_business_subscriptions_plan ON business_subscriptions(plan_id);
-- At most one live subscription per business.
CREATE UNIQUE INDEX ux_business_subscriptions_current
    ON business_subscriptions(business_id)
    WHERE status IN ('TRIAL', 'ACTIVE', 'PAST_DUE');

CREATE TABLE payment_orders (
    id                  uuid PRIMARY KEY DEFAULT uuid_v7(),
    user_id             uuid NOT NULL REFERENCES users(id),
    business_id         uuid REFERENCES businesses(id),
    -- Sum of the order's items.
    amount              numeric(14,2) NOT NULL CHECK (amount >= 0),
    currency            currency_code NOT NULL DEFAULT 'EUR',
    provider            varchar(50),
    provider_order_id   varchar(255),
    status              varchar(20) NOT NULL DEFAULT 'PENDING'
                        CHECK (status IN ('PENDING', 'AUTHORIZED', 'PAID', 'FAILED', 'CANCELLED', 'REFUNDED', 'PARTIALLY_REFUNDED')),
    paid_at             timestamptz,
    created_at          timestamptz NOT NULL DEFAULT now(),
    updated_at          timestamptz NOT NULL DEFAULT now(),
    -- A provider's order id identifies exactly one order, however often its webhook is retried.
    UNIQUE (provider, provider_order_id)
);

CREATE INDEX idx_payment_orders_user ON payment_orders(user_id, created_at DESC);
CREATE INDEX idx_payment_orders_business ON payment_orders(business_id, created_at DESC) WHERE business_id IS NOT NULL;

-- What was bought, with the price at the time of purchase.
CREATE TABLE payment_order_items (
    id                  uuid PRIMARY KEY DEFAULT uuid_v7(),
    payment_order_id    uuid NOT NULL REFERENCES payment_orders(id),
    item_type           varchar(30) NOT NULL
                        CHECK (item_type IN ('PROMOTION', 'SUBSCRIPTION', 'LISTING_FEE', 'OTHER')),
    promotion_product_id uuid REFERENCES promotion_products(id),
    business_subscription_id uuid REFERENCES business_subscriptions(id),
    listing_id          uuid REFERENCES listings(id) ON DELETE SET NULL,
    description         varchar(255) NOT NULL,
    quantity            integer NOT NULL DEFAULT 1 CHECK (quantity > 0),
    unit_amount         numeric(14,2) NOT NULL CHECK (unit_amount >= 0),
    amount              numeric(14,2) NOT NULL CHECK (amount >= 0),
    created_at          timestamptz NOT NULL DEFAULT now(),
    CHECK (item_type <> 'PROMOTION' OR promotion_product_id IS NOT NULL),
    CHECK (item_type <> 'SUBSCRIPTION' OR business_subscription_id IS NOT NULL)
);

CREATE INDEX idx_payment_order_items_order ON payment_order_items(payment_order_id);
CREATE INDEX idx_payment_order_items_listing ON payment_order_items(listing_id) WHERE listing_id IS NOT NULL;
CREATE INDEX idx_payment_order_items_subscription ON payment_order_items(business_subscription_id)
    WHERE business_subscription_id IS NOT NULL;

CREATE TABLE payment_transactions (
    id                  uuid PRIMARY KEY DEFAULT uuid_v7(),
    payment_order_id    uuid NOT NULL REFERENCES payment_orders(id),
    transaction_type    varchar(20) NOT NULL
                        CHECK (transaction_type IN ('AUTHORIZE', 'CAPTURE', 'REFUND', 'VOID', 'FAILURE')),
    amount              numeric(14,2) NOT NULL CHECK (amount >= 0),
    provider_reference  varchar(255),
    -- Provider response for support/debugging. Card numbers and CVV must never end up here.
    raw_response        jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at          timestamptz NOT NULL DEFAULT now(),
    UNIQUE (payment_order_id, transaction_type, provider_reference)
);

-- Webhook idempotency: INSERT ... ON CONFLICT DO NOTHING first; if no row was inserted the
-- event was already handled.
CREATE TABLE payment_webhook_events (
    provider            varchar(50) NOT NULL,
    provider_event_id   varchar(255) NOT NULL,
    event_type          varchar(100),
    payload             jsonb NOT NULL,
    received_at         timestamptz NOT NULL DEFAULT now(),
    processed_at        timestamptz,
    PRIMARY KEY (provider, provider_event_id)
);

CREATE TABLE listing_promotions (
    id                  uuid PRIMARY KEY DEFAULT uuid_v7(),
    listing_id          uuid NOT NULL REFERENCES listings(id) ON DELETE CASCADE,
    promotion_product_id uuid NOT NULL REFERENCES promotion_products(id),
    -- NULL when free, e.g. included in a business plan.
    payment_order_item_id uuid REFERENCES payment_order_items(id),
    starts_at           timestamptz NOT NULL,
    ends_at             timestamptz,
    status              varchar(20) NOT NULL DEFAULT 'SCHEDULED'
                        CHECK (status IN ('SCHEDULED', 'ACTIVE', 'EXPIRED', 'CANCELLED')),
    created_at          timestamptz NOT NULL DEFAULT now(),
    CHECK (ends_at IS NULL OR ends_at > starts_at)
);

CREATE INDEX idx_listing_promotions_listing ON listing_promotions(listing_id, status, starts_at DESC);
CREATE INDEX idx_listing_promotions_active ON listing_promotions(status, starts_at, ends_at);
CREATE INDEX idx_listing_promotions_order_item ON listing_promotions(payment_order_item_id)
    WHERE payment_order_item_id IS NOT NULL;

-- ============================================================
-- MODERATION / SAFETY / REPORTING
-- ============================================================

CREATE TABLE reports (
    id                  uuid PRIMARY KEY DEFAULT uuid_v7(),
    reporter_user_id    uuid REFERENCES users(id) ON DELETE SET NULL,
    listing_id          uuid REFERENCES listings(id) ON DELETE CASCADE,
    reported_user_id    uuid REFERENCES users(id),
    reported_business_id uuid REFERENCES businesses(id),
    -- Scams mostly happen in chat. A message report also names the sender in reported_user_id.
    message_id          uuid REFERENCES messages(id) ON DELETE SET NULL,
    reason_code         varchar(50) NOT NULL,
    details             text,
    status              varchar(20) NOT NULL DEFAULT 'OPEN'
                        CHECK (status IN ('OPEN', 'IN_REVIEW', 'RESOLVED', 'DISMISSED')),
    assigned_to_user_id uuid REFERENCES users(id),
    resolution          text,
    created_at          timestamptz NOT NULL DEFAULT now(),
    resolved_at         timestamptz,
    CHECK (num_nonnulls(listing_id, reported_user_id, reported_business_id) >= 1),
    CHECK (message_id IS NULL OR reported_user_id IS NOT NULL)
);

CREATE INDEX idx_reports_status ON reports(status, created_at);
CREATE INDEX idx_reports_listing ON reports(listing_id, created_at DESC);
CREATE INDEX idx_reports_reported_user ON reports(reported_user_id, created_at DESC) WHERE reported_user_id IS NOT NULL;
CREATE INDEX idx_reports_reported_business ON reports(reported_business_id) WHERE reported_business_id IS NOT NULL;
CREATE INDEX idx_reports_reporter ON reports(reporter_user_id) WHERE reporter_user_id IS NOT NULL;
CREATE INDEX idx_reports_assignee ON reports(assigned_to_user_id, status) WHERE assigned_to_user_id IS NOT NULL;
CREATE INDEX idx_reports_message ON reports(message_id) WHERE message_id IS NOT NULL;
-- One open report per person per listing.
CREATE UNIQUE INDEX ux_reports_open_per_reporter
    ON reports(reporter_user_id, listing_id)
    WHERE status IN ('OPEN', 'IN_REVIEW');

CREATE TABLE moderation_actions (
    id                  uuid PRIMARY KEY DEFAULT uuid_v7(),
    -- NULL = automated rule (auto-approval, spam filter).
    moderator_user_id   uuid REFERENCES users(id),
    report_id           uuid REFERENCES reports(id) ON DELETE SET NULL,
    listing_id          uuid REFERENCES listings(id) ON DELETE SET NULL,
    target_user_id      uuid REFERENCES users(id) ON DELETE SET NULL,
    target_business_id  uuid REFERENCES businesses(id) ON DELETE SET NULL,
    action_type         varchar(40) NOT NULL
                        CHECK (action_type IN ('APPROVE_LISTING', 'REJECT_LISTING', 'SUSPEND_LISTING', 'RESTORE_LISTING', 'WARN_USER', 'SUSPEND_USER', 'RESTORE_USER', 'SUSPEND_BUSINESS', 'RESTORE_BUSINESS')),
    reason              text,
    metadata            jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at          timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX idx_moderation_actions_listing ON moderation_actions(listing_id, created_at DESC);
CREATE INDEX idx_moderation_actions_user ON moderation_actions(target_user_id, created_at DESC);
CREATE INDEX idx_moderation_actions_moderator ON moderation_actions(moderator_user_id, created_at DESC)
    WHERE moderator_user_id IS NOT NULL;
CREATE INDEX idx_moderation_actions_report ON moderation_actions(report_id) WHERE report_id IS NOT NULL;

-- ============================================================
-- NOTIFICATIONS
-- ============================================================

CREATE TABLE notifications (
    id                  uuid PRIMARY KEY DEFAULT uuid_v7(),
    user_id             uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    notification_type   varchar(50) NOT NULL,
    title               varchar(180) NOT NULL,
    body                text,
    payload             jsonb NOT NULL DEFAULT '{}'::jsonb,
    read_at             timestamptz,
    created_at          timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX idx_notifications_user ON notifications(user_id, created_at DESC);
CREATE INDEX idx_notifications_user_unread ON notifications(user_id, created_at DESC) WHERE read_at IS NULL;

CREATE TABLE notification_preferences (
    user_id             uuid PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    email_enabled       boolean NOT NULL DEFAULT true,
    sms_enabled         boolean NOT NULL DEFAULT false,
    push_enabled        boolean NOT NULL DEFAULT true,
    marketing_enabled   boolean NOT NULL DEFAULT false,
    saved_search_alerts boolean NOT NULL DEFAULT true,
    message_alerts      boolean NOT NULL DEFAULT true,
    price_drop_alerts   boolean NOT NULL DEFAULT true,
    updated_at          timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE device_tokens (
    id                  uuid PRIMARY KEY DEFAULT uuid_v7(),
    user_id             uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    platform            varchar(20) NOT NULL CHECK (platform IN ('IOS', 'ANDROID', 'WEB')),
    token               text NOT NULL UNIQUE,
    last_seen_at        timestamptz NOT NULL DEFAULT now(),
    created_at          timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX idx_device_tokens_user ON device_tokens(user_id);

-- ============================================================
-- ANALYTICS / COUNTERS
-- ============================================================

-- Buffer increments (e.g. in Redis) and flush periodically; one UPDATE per page view turns
-- popular listings into lock hot spots.
CREATE TABLE listing_daily_stats (
    listing_id          uuid NOT NULL REFERENCES listings(id) ON DELETE CASCADE,
    stat_date           date NOT NULL,
    impressions         bigint NOT NULL DEFAULT 0 CHECK (impressions >= 0),
    detail_views        bigint NOT NULL DEFAULT 0 CHECK (detail_views >= 0),
    favorites           bigint NOT NULL DEFAULT 0 CHECK (favorites >= 0),
    messages_started    bigint NOT NULL DEFAULT 0 CHECK (messages_started >= 0),
    phone_reveals       bigint NOT NULL DEFAULT 0 CHECK (phone_reveals >= 0),
    website_clicks      bigint NOT NULL DEFAULT 0 CHECK (website_clicks >= 0),
    PRIMARY KEY (listing_id, stat_date)
);

CREATE INDEX idx_listing_daily_stats_date ON listing_daily_stats(stat_date);

-- ============================================================
-- OUTBOX
-- ============================================================

-- Insert an event in the same transaction as the change it describes; a worker (polling with
-- FOR UPDATE SKIP LOCKED) or Debezium publishes it. This keeps OpenSearch, notifications and
-- saved-search alerts in sync without dual writes.
CREATE TABLE outbox_events (
    id                  bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    aggregate_type      varchar(60) NOT NULL,
    aggregate_id        uuid NOT NULL,
    event_type          varchar(100) NOT NULL,
    payload             jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at          timestamptz NOT NULL DEFAULT now(),
    published_at        timestamptz
);

CREATE INDEX idx_outbox_events_unpublished ON outbox_events(id) WHERE published_at IS NULL;

-- ============================================================
-- AUDIT LOG
-- ============================================================

CREATE TABLE audit_log (
    id                  bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    actor_user_id       uuid REFERENCES users(id) ON DELETE SET NULL,
    entity_type         varchar(60) NOT NULL,
    entity_id           uuid,
    action              varchar(60) NOT NULL,
    ip_address          inet,
    user_agent          text,
    old_values          jsonb,
    new_values          jsonb,
    created_at          timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX idx_audit_log_entity ON audit_log(entity_type, entity_id, created_at DESC);
CREATE INDEX idx_audit_log_actor ON audit_log(actor_user_id, created_at DESC);

-- ============================================================
-- TRIGGERS
-- ============================================================

-- Trigger functions qualify table names because they run with the caller's search_path.

-- The application can attribute a status change by running, in the same transaction:
--   SELECT set_config('app.user_id', '<uuid>', true), set_config('app.status_reason', '<text>', true);
-- Changes made without it (e.g. the expiry job) are recorded with changed_by_user_id NULL.
CREATE OR REPLACE FUNCTION record_listing_status_change()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    INSERT INTO public.listing_status_history (listing_id, from_status, to_status, reason, changed_by_user_id)
    VALUES (
        NEW.id,
        CASE WHEN TG_OP = 'UPDATE' THEN OLD.status END,
        NEW.status,
        nullif(current_setting('app.status_reason', true), ''),
        nullif(current_setting('app.user_id', true), '')::uuid
    );
    RETURN NULL;
END;
$$;

CREATE TRIGGER trg_listings_status_history_insert
AFTER INSERT ON listings
FOR EACH ROW EXECUTE FUNCTION record_listing_status_change();

CREATE TRIGGER trg_listings_status_history_update
AFTER UPDATE OF status ON listings
FOR EACH ROW WHEN (OLD.status IS DISTINCT FROM NEW.status)
EXECUTE FUNCTION record_listing_status_change();

CREATE OR REPLACE FUNCTION record_listing_price_change()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    INSERT INTO public.listing_price_history (listing_id, price_amount, currency, price_type)
    VALUES (NEW.id, NEW.price_amount, NEW.currency, NEW.price_type);
    RETURN NULL;
END;
$$;

CREATE TRIGGER trg_listings_price_history_insert
AFTER INSERT ON listings
FOR EACH ROW EXECUTE FUNCTION record_listing_price_change();

CREATE TRIGGER trg_listings_price_history_update
AFTER UPDATE OF price_amount, currency, price_type ON listings
FOR EACH ROW WHEN (OLD.price_amount IS DISTINCT FROM NEW.price_amount
                OR OLD.currency     IS DISTINCT FROM NEW.currency
                OR OLD.price_type   IS DISTINCT FROM NEW.price_type)
EXECUTE FUNCTION record_listing_price_change();

CREATE TRIGGER trg_users_updated_at
BEFORE UPDATE ON users FOR EACH ROW EXECUTE FUNCTION set_updated_at();

CREATE TRIGGER trg_user_verifications_updated_at
BEFORE UPDATE ON user_verifications FOR EACH ROW EXECUTE FUNCTION set_updated_at();

CREATE TRIGGER trg_businesses_updated_at
BEFORE UPDATE ON businesses FOR EACH ROW EXECUTE FUNCTION set_updated_at();

CREATE TRIGGER trg_categories_updated_at
BEFORE UPDATE ON categories FOR EACH ROW EXECUTE FUNCTION set_updated_at();

CREATE TRIGGER trg_attributes_updated_at
BEFORE UPDATE ON attributes FOR EACH ROW EXECUTE FUNCTION set_updated_at();

CREATE TRIGGER trg_listings_updated_at
BEFORE UPDATE ON listings FOR EACH ROW EXECUTE FUNCTION set_updated_at();

CREATE TRIGGER trg_listing_attribute_values_updated_at
BEFORE UPDATE ON listing_attribute_values FOR EACH ROW EXECUTE FUNCTION set_updated_at();

CREATE TRIGGER trg_saved_searches_updated_at
BEFORE UPDATE ON saved_searches FOR EACH ROW EXECUTE FUNCTION set_updated_at();

CREATE TRIGGER trg_conversations_updated_at
BEFORE UPDATE ON conversations FOR EACH ROW EXECUTE FUNCTION set_updated_at();

CREATE TRIGGER trg_reviews_updated_at
BEFORE UPDATE ON reviews FOR EACH ROW EXECUTE FUNCTION set_updated_at();

CREATE TRIGGER trg_payment_orders_updated_at
BEFORE UPDATE ON payment_orders FOR EACH ROW EXECUTE FUNCTION set_updated_at();

CREATE TRIGGER trg_business_subscriptions_updated_at
BEFORE UPDATE ON business_subscriptions FOR EACH ROW EXECUTE FUNCTION set_updated_at();

CREATE TRIGGER trg_notification_preferences_updated_at
BEFORE UPDATE ON notification_preferences FOR EACH ROW EXECUTE FUNCTION set_updated_at();
