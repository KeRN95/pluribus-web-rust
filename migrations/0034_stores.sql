-- Stores (web/stores.rs): a business someone makes from their account, which their listings can be
-- posted as and which has a page of its own (/biznes/{slug}): where it is (its town and address),
-- when it is open, whether its public number takes WhatsApp, and the files of its logo and cover
-- in the photo store, to take out when they are replaced.
ALTER TABLE businesses ADD COLUMN location_id uuid REFERENCES locations(id);
ALTER TABLE businesses ADD COLUMN address varchar(200);
ALTER TABLE businesses ADD COLUMN opening_hours varchar(300);
ALTER TABLE businesses ADD COLUMN whatsapp boolean NOT NULL DEFAULT false;
ALTER TABLE businesses ADD COLUMN logo_key text;
ALTER TABLE businesses ADD COLUMN cover_key text;
