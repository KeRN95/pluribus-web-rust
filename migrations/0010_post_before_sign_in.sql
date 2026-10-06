-- Posting before signing in: the listing waits as a DRAFT, under an account created for its
-- contact (PENDING until confirmed), and the code sent to that contact remembers it, so typing the
-- code back confirms the contact and publishes the listing in one step. A name typed when creating
-- an account waits on the code too.
ALTER TABLE login_codes ADD COLUMN listing_id uuid REFERENCES listings(id) ON DELETE CASCADE;
ALTER TABLE login_codes ADD COLUMN display_name varchar(120);
