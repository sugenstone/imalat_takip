-- Faz 5: Workflow Runtime
-- Is kalemine atanan PUBLISHED versiyondan bagimsiz instance uretilir.
-- Surec motoru: waiting -> ready -> in_progress -> submitted -> approved (MVP: onaysiz approved)
-- current_attempt_id Faz 7'ye (rework) hazirliktir.

CREATE TABLE workflow_instances (
    id                  TEXT PRIMARY KEY,
    work_item_id        TEXT NOT NULL REFERENCES work_items(id),
    workflow_version_id TEXT NOT NULL REFERENCES workflow_versions(id),
    status              TEXT NOT NULL DEFAULT 'active', -- active | completed | cancelled
    started_at          TEXT,
    completed_at        TEXT,
    created_at          TEXT NOT NULL
);
CREATE INDEX idx_wfi_work_item ON workflow_instances(work_item_id, status);

CREATE TABLE process_instances (
    id                   TEXT PRIMARY KEY,
    workflow_instance_id TEXT NOT NULL REFERENCES workflow_instances(id) ON DELETE CASCADE,
    workflow_node_id     TEXT NOT NULL REFERENCES workflow_nodes(id),
    status               TEXT NOT NULL DEFAULT 'waiting',
                         -- waiting | ready | in_progress | submitted | approved | failed | cancelled | skipped
    ready_at             TEXT,
    started_at           TEXT,
    finished_at          TEXT,
    current_attempt_id   TEXT, -- Faz 7: rework attempt zinciri
    created_at           TEXT NOT NULL
);
CREATE INDEX idx_pi_instance ON process_instances(workflow_instance_id);
CREATE INDEX idx_pi_status  ON process_instances(status);
