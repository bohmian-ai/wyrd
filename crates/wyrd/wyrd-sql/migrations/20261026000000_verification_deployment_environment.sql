-- Deployment environment correlation tag of accepted verification work,
-- written unchanged to the execution's result rows.
ALTER TABLE wyrd.verifier_executions
    ADD COLUMN deployment_environment TEXT
        CHECK (deployment_environment IS NULL OR octet_length(deployment_environment) > 0);
