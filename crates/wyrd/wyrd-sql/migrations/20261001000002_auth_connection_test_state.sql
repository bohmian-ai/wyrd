-- Candidate connection tests sign in for real.
--
-- Testing a candidate begins one authorization-code login through the common
-- callback, so its state is an ordinary login-state row with the same
-- single-use, expiry, and issuer binding. Its initiation binding is the
-- principal that began the test instead of a browser flow or device authorization: the
-- callback re-checks that principal's authority before it marks the bound
-- candidate revision tested. A test issues nothing, so its row never carries
-- a completion.
ALTER TABLE wyrd.auth_login_state
    ADD COLUMN tester_principal_id   UUID,
    ADD COLUMN tester_principal_kind TEXT;

ALTER TABLE wyrd.auth_login_state DROP CONSTRAINT auth_login_state_check;

ALTER TABLE wyrd.auth_login_state
    ADD CONSTRAINT auth_login_state_initiation
        CHECK (num_nonnulls(browser_flow_hash, device_id, tester_principal_id) = 1),
    ADD CONSTRAINT auth_login_state_tester_kind
        CHECK ((tester_principal_id IS NULL) = (tester_principal_kind IS NULL)),
    ADD CONSTRAINT auth_login_state_test_never_completes
        CHECK (tester_principal_id IS NULL OR completion_sealed IS NULL);
