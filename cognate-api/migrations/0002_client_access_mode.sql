ALTER TABLE clients ADD COLUMN access_mode TEXT NOT NULL DEFAULT 'read_write';

CREATE INDEX IF NOT EXISTS clients_access_mode_idx
    ON clients (access_mode, revoked_at);
