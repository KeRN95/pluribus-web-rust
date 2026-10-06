-- Sellers can take a listing down for a while and put it back: PAUSED, seen only by its seller,
-- on "Njoftimet e mia". Deleting a listing marks it DELETED, as before; it then shows nowhere.
ALTER TABLE listings DROP CONSTRAINT listings_status_check;
ALTER TABLE listings ADD CONSTRAINT listings_status_check
    CHECK (status IN ('DRAFT', 'PENDING_REVIEW', 'ACTIVE', 'RESERVED', 'SOLD', 'EXPIRED', 'REJECTED',
                      'SUSPENDED', 'PAUSED', 'DELETED'));
