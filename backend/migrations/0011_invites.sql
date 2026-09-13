-- E-posta davet sistemi: kayitli OLMAYAN kullaniciyi token'li davet linkiyle cagirma.
-- Token 64 haneli hex, 7 gun gecerli, tek kullanimli, iptal edilebilir.

CREATE TABLE workspace_invites (
    id           TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL REFERENCES workspaces(id),
    email        TEXT NOT NULL,
    role_id      TEXT NOT NULL REFERENCES roles(id),
    token        TEXT NOT NULL UNIQUE,
    status       TEXT NOT NULL DEFAULT 'pending', -- pending | accepted | cancelled
    expires_at   TEXT NOT NULL,
    invited_by   TEXT NOT NULL REFERENCES users(id),
    created_at   TEXT NOT NULL,
    accepted_at  TEXT
);
CREATE INDEX idx_invites_workspace ON workspace_invites(workspace_id, status);
CREATE INDEX idx_invites_email ON workspace_invites(email);
