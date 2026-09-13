-- Faz 9: Bildirim + e-posta kuyrugu (yol haritasi 37-39)
-- Mail request icinde gonderilmez: email_outbox kuyrugu -> worker -> provider.

CREATE TABLE notifications (
    id           TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL REFERENCES workspaces(id),
    user_id      TEXT NOT NULL REFERENCES users(id),
    type         TEXT NOT NULL,
                 -- process.assigned | process.approval_required | process.approved
                 -- | process.rejected | rework.started
    title        TEXT NOT NULL,
    message      TEXT NOT NULL,
    entity_type  TEXT,
    entity_id    TEXT,
    read_at      TEXT,
    created_at   TEXT NOT NULL
);
CREATE INDEX idx_notifications_user ON notifications(user_id, read_at);
CREATE INDEX idx_notifications_workspace ON notifications(workspace_id, created_at DESC);

CREATE TABLE email_outbox (
    id           TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL REFERENCES workspaces(id),
    to_email     TEXT NOT NULL,
    subject      TEXT NOT NULL,
    body         TEXT NOT NULL,
    status       TEXT NOT NULL DEFAULT 'pending', -- pending | sent | failed
    created_at   TEXT NOT NULL,
    sent_at      TEXT
);
CREATE INDEX idx_outbox_status ON email_outbox(status, created_at);
