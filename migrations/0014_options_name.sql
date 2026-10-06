-- A car's equipment (ABS, air conditioning, navigation...) is called "Opsionet": in the posting
-- form, in the filters and on the listing.
UPDATE attributes
SET name = 'Opsionet', name_i18n = name_i18n || '{"en": "Options"}', updated_at = now()
WHERE code = 'vehicle_features';
