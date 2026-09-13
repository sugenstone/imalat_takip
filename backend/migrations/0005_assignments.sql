-- Faz 6: Atama + Onay katmani
-- submit != approve (yol haritasi 26-27): kurali olan node'lar submitted'da onay bekler.
-- Atama gecmisi silinmez (28): unassigned_at ile sonlanir.

CREATE TABLE process_assignments (
    id                  TEXT PRIMARY KEY,
    process_instance_id TEXT NOT NULL REFERENCES process_instances(id) ON DELETE CASCADE,
    assignee_type       TEXT NOT NULL, -- user | team (ileri: role)
    assignee_id         TEXT NOT NULL,
    assigned_by         TEXT NOT NULL REFERENCES users(id),
    assigned_at         TEXT NOT NULL,
    unassigned_at       TEXT
);
CREATE INDEX idx_pa_process ON process_assignments(process_instance_id);
CREATE INDEX idx_pa_assignee ON process_assignments(assignee_type, assignee_id);

CREATE TABLE approvals (
    id                  TEXT PRIMARY KEY,
    process_instance_id TEXT NOT NULL REFERENCES process_instances(id) ON DELETE CASCADE,
    process_attempt_id  TEXT, -- Faz 7'ye hazirlik
    requested_by        TEXT NOT NULL REFERENCES users(id),
    requested_at        TEXT NOT NULL,
    decided_by          TEXT REFERENCES users(id),
    decided_at          TEXT,
    decision            TEXT NOT NULL DEFAULT 'pending', -- pending | approved | rejected
    note                TEXT
);
CREATE INDEX idx_appr_process ON approvals(process_instance_id);
CREATE INDEX idx_appr_decision ON approvals(decision);
