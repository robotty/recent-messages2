DO $$ 
DECLARE
    is_installed BOOLEAN;
BEGIN
    SELECT EXISTS (
        SELECT 1 FROM pg_extension WHERE extname = 'timescaledb'
    ) INTO is_installed;

    IF NOT is_installed THEN
        RAISE EXCEPTION 'TimescaleDB extension is not installed. Did you follow the instructions in the README?';
    END IF;
END $$;

SELECT create_hypertable(
    relation => 'message',
    dimension => by_range('time_received', INTERVAL '1 hour'),
    create_default_indexes => false, -- we already have an index
    migrate_data => true
);
-- we intentionally do not enable Hypercore (column storage) since the benefit is dubious in our use case
