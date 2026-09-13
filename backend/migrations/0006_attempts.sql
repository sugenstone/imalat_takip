-- Faz 7: Rework / Attempt zinciri
-- Ilke (yol haritasi 29-30): basarisiz surec geriye CEVRILMEZ, approved kayit
-- overwrite EDILMEZ. Her deneme ayri process_attempt; eskiler superseded kalir.

CREATE TABLE process_attempts (
    id                  TEXT PRIMARY KEY,
    process_instance_id TEXT NOT NULL REFERENCES process_instances(id) ON DELETE CASCADE,
    attempt_number      INTEGER NOT NULL,
    status              TEXT NOT NULL DEFAULT 'in_progress',
                        -- in_progress | submitted | approved | failed | superseded
    started_at          TEXT,
    submitted_at        TEXT,
    approved_at         TEXT,
    failed_at           TEXT,
    completed_by        TEXT REFERENCES users(id),
    failure_reason      TEXT,
    created_at          TEXT NOT NULL,
    UNIQUE(process_instance_id, attempt_number)
);
CREATE INDEX idx_patt_instance ON process_attempts(process_instance_id);
CREATE INDEX idx_patt_status ON process_attempts(status);

CREATE TABLE rework_cycles (
    id                       TEXT PRIMARY KEY,
    workflow_instance_id     TEXT NOT NULL REFERENCES workflow_instances(id) ON DELETE CASCADE,
    triggered_by_process_id  TEXT NOT NULL REFERENCES process_instances(id),
    restart_from_process_id  TEXT NOT NULL REFERENCES process_instances(id),
    reason                   TEXT NOT NULL,
    created_by               TEXT NOT NULL REFERENCES users(id),
    created_at               TEXT NOT NULL
);
CREATE INDEX idx_rc_instance ON rework_cycles(workflow_instance_id);
