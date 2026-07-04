-- Repair OLAP tenant policies for databases that applied an early
-- 20260619000001_olap_minimal migration without the table policies present.

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_policies
        WHERE schemaname = 'vala'
          AND tablename = 'bifrost_tables'
          AND policyname = 'tenant_isolation'
    ) THEN
        EXECUTE 'CREATE POLICY tenant_isolation ON vala.bifrost_tables
            USING      (data_tenant_id = wyrd.current_tenant())
            WITH CHECK (data_tenant_id = wyrd.current_tenant())';
    END IF;

    IF NOT EXISTS (
        SELECT 1
        FROM pg_policies
        WHERE schemaname = 'vala'
          AND tablename = 'olap_commits'
          AND policyname = 'tenant_isolation'
    ) THEN
        EXECUTE 'CREATE POLICY tenant_isolation ON vala.olap_commits
            USING      (data_tenant_id = wyrd.current_tenant())
            WITH CHECK (data_tenant_id = wyrd.current_tenant())';
    END IF;

    IF NOT EXISTS (
        SELECT 1
        FROM pg_policies
        WHERE schemaname = 'vala'
          AND tablename = 'refresh_epochs'
          AND policyname = 'tenant_isolation'
    ) THEN
        EXECUTE 'CREATE POLICY tenant_isolation ON vala.refresh_epochs
            USING      (data_tenant_id = wyrd.current_tenant())
            WITH CHECK (data_tenant_id = wyrd.current_tenant())';
    END IF;
END $$;
