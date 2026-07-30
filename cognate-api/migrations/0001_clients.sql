CREATE TABLE IF NOT EXISTS clients (
    id           TEXT PRIMARY KEY NOT NULL,
    client_name  TEXT NOT NULL,
    key_hash     TEXT NOT NULL UNIQUE,
    created_at   TEXT NOT NULL,
    revoked_at   TEXT,
    last_used_at TEXT
);

CREATE INDEX IF NOT EXISTS clients_active_idx
    ON clients (revoked_at, key_hash);
