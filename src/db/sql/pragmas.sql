-- WAL grants faster performance and allows reads that are concurrent to writes.
PRAGMA journal_mode = WAL;
-- NORMAL synchronization is safe with WAL enabled, and gives an extra speed boost
-- by minimizing filesystem IO.
PRAGMA synchronous = NORMAL;
PRAGMA foreign_keys = ON;