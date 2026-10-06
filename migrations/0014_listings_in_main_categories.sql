-- Listings go into a main category too, not only into its subcategories, for what none of them
-- fits. Such a listing shows on its category's page and those above it, under none of the
-- subcategories, and has its category's properties only.
UPDATE categories SET allow_listings = true, updated_at = now() WHERE NOT allow_listings;
