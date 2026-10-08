-- The planner learns that an option names its property. A filter on a car's make and its fuel
-- reads `attribute_id = make AND value_option_id = Volkswagen` for each; judging the two columns
-- apart, the planner multiplied their odds, though an option belongs to one property alone, and
-- took a make for a thousandth of the values when it is nearly one in a hundred (2 300 Volkswagens
-- for the 38 853 there are, 6 000 diesels for 119 480). So it joined every Volkswagen diesel to
-- pick the 25 newest (115 ms), where walking the newest cars until 25 match takes 17 ms.
-- Functional-dependency statistics on the two columns say so; they count from the next ANALYZE,
-- run here so they count at once.

CREATE STATISTICS listing_attribute_values_option_dependency (dependencies)
    ON attribute_id, value_option_id FROM listing_attribute_values;

ANALYZE listing_attribute_values;
