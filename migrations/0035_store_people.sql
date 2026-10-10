-- A store's people (web/stores.rs). Its owner and admins ask someone in by a phone number or an
-- e-mail address, with the role they will have (business_members.role, the owner's aside), and
-- the invitation waits for whoever signs in with that contact to take it or turn it down, until
-- it is withdrawn or two weeks pass. One per store and contact: asked again, it is the same one,
-- with the new role and two weeks again.
CREATE TABLE business_invitations (
    id                  uuid PRIMARY KEY DEFAULT uuid_v7(),
    business_id         uuid NOT NULL REFERENCES businesses(id) ON DELETE CASCADE,
    phone_e164          e164_phone,
    email               citext,
    role                varchar(30) NOT NULL CHECK (role IN ('ADMIN', 'EDITOR', 'VIEWER')),
    invited_by_user_id  uuid REFERENCES users(id) ON DELETE SET NULL,
    created_at          timestamptz NOT NULL DEFAULT now(),
    expires_at          timestamptz NOT NULL DEFAULT now() + interval '14 days',
    CHECK (num_nonnulls(phone_e164, email) = 1)
);

CREATE UNIQUE INDEX ux_business_invitations_phone ON business_invitations(business_id, phone_e164)
    WHERE phone_e164 IS NOT NULL;
CREATE UNIQUE INDEX ux_business_invitations_email ON business_invitations(business_id, email)
    WHERE email IS NOT NULL;
-- The invitations a user's contacts have, from any store.
CREATE INDEX idx_business_invitations_phone ON business_invitations(phone_e164) WHERE phone_e164 IS NOT NULL;
CREATE INDEX idx_business_invitations_email ON business_invitations(email) WHERE email IS NOT NULL;

-- Whether `who` may change a listing: a private seller's own (the account that posted it), or a
-- store's for its people who post, its owner, admins and editors, whichever of them posted it;
-- not for someone who has left the store, whatever they posted for it.
CREATE FUNCTION manages_listing(seller_business uuid, created_by uuid, who uuid) RETURNS boolean
    LANGUAGE sql STABLE PARALLEL SAFE
AS $$
    SELECT CASE WHEN seller_business IS NULL THEN created_by = who
                ELSE EXISTS (SELECT 1 FROM business_members m
                             WHERE m.business_id = seller_business AND m.user_id = who
                               AND m.role IN ('OWNER', 'ADMIN', 'EDITOR'))
           END
$$;
