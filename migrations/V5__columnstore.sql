ALTER TABLE message SET (timescaledb.segmentby = 'channel_login');
CALL add_columnstore_policy('message', after => INTERVAL '1 hour'); -- same as chunk size in V4__timescaledb.sql
