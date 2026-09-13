-- Faz 4: Workflow Designer
-- Template (taslak container) -> Version (draft/published/retired) -> Node -> Dependency (DAG)
-- Published versiyon IMMUTABLE: degisiklik yeni versiyon uretir.

CREATE TABLE workflow_templates (
    id           TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL REFERENCES workspaces(id),
    name         TEXT NOT NULL,
    description  TEXT,
    status       TEXT NOT NULL DEFAULT 'active',
    created_by   TEXT NOT NULL REFERENCES users(id),
    created_at   TEXT NOT NULL,
    updated_at   TEXT NOT NULL,
    archived_at  TEXT,
    UNIQUE(workspace_id, name)
);

CREATE TABLE workflow_versions (
    id              TEXT PRIMARY KEY,
    template_id     TEXT NOT NULL REFERENCES workflow_templates(id),
    version_number  INTEGER NOT NULL,
    status          TEXT NOT NULL DEFAULT 'draft', -- draft | published | retired
    published_at    TEXT,
    published_by    TEXT REFERENCES users(id),
    created_at      TEXT NOT NULL,
    UNIQUE(template_id, version_number)
);
CREATE INDEX idx_wf_versions_template ON workflow_versions(template_id, status);

CREATE TABLE workflow_nodes (
    id                 TEXT PRIMARY KEY,
    version_id         TEXT NOT NULL REFERENCES workflow_versions(id),
    name               TEXT NOT NULL,
    description        TEXT,
    node_type          TEXT NOT NULL DEFAULT 'process', -- MVP: process (ileri: decision/gate/automation)
    sort_order         INTEGER NOT NULL DEFAULT 0,
    default_duration   TEXT, -- sure bilgisi (opsiyonel)
    approval_rule_json TEXT, -- Faz 6 hazirlik
    created_at         TEXT NOT NULL
);
CREATE INDEX idx_wf_nodes_version ON workflow_nodes(version_id);

CREATE TABLE workflow_dependencies (
    id                  TEXT PRIMARY KEY,
    version_id          TEXT NOT NULL REFERENCES workflow_versions(id),
    predecessor_node_id TEXT NOT NULL REFERENCES workflow_nodes(id) ON DELETE CASCADE,
    successor_node_id   TEXT NOT NULL REFERENCES workflow_nodes(id) ON DELETE CASCADE,
    dependency_type     TEXT NOT NULL DEFAULT 'all_completed', -- MVP: all_completed
    created_at          TEXT NOT NULL,
    UNIQUE(predecessor_node_id, successor_node_id)
);
CREATE INDEX idx_wf_deps_version ON workflow_dependencies(version_id);
