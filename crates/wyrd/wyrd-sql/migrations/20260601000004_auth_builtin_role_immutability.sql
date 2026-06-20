-- Builtin auth role names are stable JWT/audit identifiers.

CREATE OR REPLACE FUNCTION wyrd.assert_builtin_role_name_immutable()
RETURNS TRIGGER AS $$
BEGIN
    IF OLD.builtin = true AND NEW.name <> OLD.name THEN
        RAISE EXCEPTION
            USING ERRCODE = '23514',
                  CONSTRAINT = 'auth_builtin_role_immutable_name',
                  MESSAGE = format(
                      'cannot rename builtin role %L to %L; builtin role names are immutable',
                      OLD.name, NEW.name
                  );
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER auth_roles_builtin_name_immutable
    BEFORE UPDATE ON wyrd.auth_roles
    FOR EACH ROW
    WHEN (OLD.builtin = true AND NEW.name IS DISTINCT FROM OLD.name)
    EXECUTE FUNCTION wyrd.assert_builtin_role_name_immutable();
