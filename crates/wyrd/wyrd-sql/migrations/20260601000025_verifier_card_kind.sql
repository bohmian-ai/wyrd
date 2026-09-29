-- Verifier replaces the unshipped Drift and Eval Card kinds.
-- Nothing registered those kinds in a shipped release, so no rows migrate.
ALTER TABLE wyrd.cards DROP CONSTRAINT cards_kind_check;
ALTER TABLE wyrd.cards ADD CONSTRAINT cards_kind_check CHECK (
    kind IN (
        'Data','Model','Experiment','Prompt','Agent','Workflow','Verifier',
        'Service','Policy','Mcp','Audit','Artifact','Trigger','Operator','Source','External'
    )
);
