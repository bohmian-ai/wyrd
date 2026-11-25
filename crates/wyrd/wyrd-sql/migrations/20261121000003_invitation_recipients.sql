-- Addressed invitation links (authorization-model spec revision 4,
-- "Persistence and authority changes": wyrd.auth_user_invitations). Each
-- invitation names its recipient address and the outcome of emailing its
-- current link; resend replaces code_hash, created_at and expires_at.
-- Existing invitations name no recipient and cannot be expressed; nothing
-- has shipped and nothing references them, so they are removed.
DELETE FROM wyrd.auth_user_invitations;

ALTER TABLE wyrd.auth_user_invitations
    ADD COLUMN recipient TEXT NOT NULL
        CONSTRAINT auth_user_invitations_recipient
        CHECK (char_length(recipient) BETWEEN 3 AND 254),
    ADD COLUMN delivery TEXT NOT NULL DEFAULT 'not_emailed'
        CONSTRAINT auth_user_invitations_delivery
        CHECK (delivery IN ('not_emailed', 'accepted', 'failed'));
