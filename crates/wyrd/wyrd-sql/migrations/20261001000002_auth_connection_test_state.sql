-- Candidate connection tests sign in for real.
--
-- Testing a candidate begins one authorization-code login through the common
-- callback, so its state is an ordinary login-state row with the same
-- single-use, expiry, and issuer binding. Its initiation binding is the
-- principal that began the test instead of an OAuth client or device
-- authorization: the callback re-checks that principal's authority before it
-- marks the bound candidate revision tested. A test issues nothing, and only
-- an OAuth client's login carries a code.
ALTER TABLE wyrd.auth_login_state
    ADD COLUMN tester_principal_id   UUID,
    ADD COLUMN tester_principal_kind TEXT;

ALTER TABLE wyrd.auth_login_state DROP CONSTRAINT auth_login_state_initiation;

ALTER TABLE wyrd.auth_login_state
    ADD CONSTRAINT auth_login_state_initiation
        CHECK (num_nonnulls(oauth_client_id, device_id, tester_principal_id) = 1),
    ADD CONSTRAINT auth_login_state_tester_kind
        CHECK ((tester_principal_id IS NULL) = (tester_principal_kind IS NULL));
