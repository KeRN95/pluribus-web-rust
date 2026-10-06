-- Sellers can add an option a list lacks: a car make or model, a phone brand. They pick "Tjetër"
-- and type it. It is stored as an option of its own, inactive: the listing shows it and search
-- finds it, but it isn't offered in the form or the filters until someone sets it active.
-- Typing an option that exists (active or not) reuses it, so the same make isn't added twice.

-- Which properties take a typed option.
ALTER TABLE attributes ADD COLUMN allow_other boolean NOT NULL DEFAULT false;

UPDATE attributes SET allow_other = true WHERE code IN ('vehicle_make', 'vehicle_model', 'electronics_brand');

-- Who added an option, for reviewing them; NULL for the site's own.
ALTER TABLE attribute_options ADD COLUMN added_by_user_id uuid REFERENCES users(id);

-- The added options waiting for review.
CREATE INDEX idx_attribute_options_added ON attribute_options (attribute_id) WHERE added_by_user_id IS NOT NULL AND NOT active;
