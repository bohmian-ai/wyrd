-- Repository Cards, their pull_request bindings and their selected policy.
--
-- A Repository is an ordinary Card. Exactly one version of a Repository
-- family may be selected: `selected_at` marks it, and the partial unique
-- index refuses a second. An Active version is not selected until the
-- declaration that targeted it settles, so Card state and selected policy
-- stay distinct.
--
-- A declaration whose root is a Repository records its target version and
-- the version selected when it was written. It settles once: when every Card
-- it owns is Active it selects its target if the family still holds the
-- expected version, reuses an identical selection, and otherwise records a
-- conflict. A settled operation never selects again, so a delayed or
-- replayed completion cannot undo a newer selection.

ALTER TABLE wyrd.cards DROP CONSTRAINT cards_kind_check;
ALTER TABLE wyrd.cards ADD CONSTRAINT cards_kind_check CHECK (
    kind IN (
        'Data','Model','Experiment','Prompt','Agent','Workflow','Verifier',
        'Service','Policy','Mcp','Audit','Artifact','Trigger','Operator','Source','External',
        'Repository'
    )
);

ALTER TABLE wyrd.cards
    ADD COLUMN selected_at TIMESTAMPTZ,
    ADD CONSTRAINT cards_selected_repository CHECK (
        selected_at IS NULL OR (kind = 'Repository' AND status = 'active')
    );

CREATE UNIQUE INDEX cards_one_selected_version
    ON wyrd.cards (data_tenant_id, kind, name)
    WHERE selected_at IS NOT NULL;

ALTER TABLE wyrd.card_declaration_operations
    ADD COLUMN selection_target_uid UUID,
    ADD COLUMN selection_expected_uid UUID,
    ADD COLUMN selection TEXT,
    ADD CONSTRAINT card_declaration_operations_selection CHECK (
        (selection IS NULL AND selection_target_uid IS NULL AND selection_expected_uid IS NULL)
        OR (selection IN ('pending', 'selected', 'conflict') AND selection_target_uid IS NOT NULL)
    );

ALTER TABLE wyrd.verification_bindings
    DROP CONSTRAINT verification_bindings_owner_card_kind_check,
    ADD CONSTRAINT verification_bindings_owner_card_kind_check
        CHECK (owner_card_kind IN ('Service', 'Agent', 'Repository')),
    DROP CONSTRAINT verification_bindings_activation_check,
    ADD CONSTRAINT verification_bindings_activation_check
        CHECK (activation IN ('schedule', 'observations_ready', 'group_end', 'pull_request')),
    DROP CONSTRAINT verification_bindings_check1,
    ADD CONSTRAINT verification_bindings_activation_settings CHECK (
        (activation = 'schedule' AND schedule_cron IS NOT NULL AND group_end IS NULL)
        OR (activation IN ('observations_ready', 'pull_request') AND schedule_cron IS NULL
            AND schedule_tz IS NULL AND next_run_at IS NULL AND group_end IS NULL)
        OR (activation = 'group_end' AND schedule_cron IS NULL
            AND schedule_tz IS NULL AND next_run_at IS NULL
            AND jsonb_typeof(group_end) = 'object')
    ),
    ADD CONSTRAINT verification_bindings_pull_request_owner CHECK (
        (activation = 'pull_request') = (owner_card_kind = 'Repository')
    ),
    DROP CONSTRAINT verification_bindings_live_activation,
    ADD CONSTRAINT verification_bindings_live_activation CHECK (
        activation IN ('schedule', 'group_end', 'pull_request')
    ) NOT VALID;
