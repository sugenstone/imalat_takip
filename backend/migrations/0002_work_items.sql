-- Faz 3: Is kalemleri + dinamik ozellikler
-- Ilke: ozellik TANIMI ve ozellik DEGERI ayri tablolarda (yol haritasi 17).
-- Tek JSON kolonuna her seyi koymak yok (56.1).

CREATE TABLE work_types (
    id           TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL REFERENCES workspaces(id),
    name         TEXT NOT NULL,
    description  TEXT,
    is_active    INTEGER NOT NULL DEFAULT 1,
    created_at   TEXT NOT NULL,
    updated_at   TEXT NOT NULL,
    archived_at  TEXT,
    UNIQUE(workspace_id, name)
);

CREATE TABLE work_items (
    id            TEXT PRIMARY KEY,
    workspace_id  TEXT NOT NULL REFERENCES workspaces(id),
    section_id    TEXT NOT NULL REFERENCES sections(id),
    work_type_id  TEXT REFERENCES work_types(id),
    name          TEXT NOT NULL,
    description   TEXT,
    priority      TEXT NOT NULL DEFAULT 'medium', -- low|medium|high|urgent
    status        TEXT NOT NULL DEFAULT 'draft',  -- draft|active|blocked|completed|cancelled
    planned_start TEXT,
    planned_end   TEXT,
    created_by    TEXT NOT NULL REFERENCES users(id),
    created_at    TEXT NOT NULL,
    updated_at    TEXT NOT NULL,
    archived_at   TEXT
);
CREATE INDEX idx_work_items_workspace ON work_items(workspace_id, archived_at);
CREATE INDEX idx_work_items_section  ON work_items(section_id);
CREATE INDEX idx_work_items_type     ON work_items(work_type_id);
CREATE INDEX idx_work_items_status   ON work_items(workspace_id, status);

CREATE TABLE work_attribute_definitions (
    id             TEXT PRIMARY KEY,
    workspace_id   TEXT NOT NULL REFERENCES workspaces(id),
    work_type_id   TEXT NOT NULL REFERENCES work_types(id),
    name           TEXT NOT NULL,
    key            TEXT NOT NULL,
    data_type      TEXT NOT NULL, -- text|textarea|integer|decimal|boolean|date|datetime|select|multiselect|email|phone|currency|percentage
    unit           TEXT,
    is_required    INTEGER NOT NULL DEFAULT 0,
    default_value  TEXT,
    sort_order     INTEGER NOT NULL DEFAULT 0,
    validation_json TEXT,
    is_filterable  INTEGER NOT NULL DEFAULT 0,
    is_active      INTEGER NOT NULL DEFAULT 1,
    created_at     TEXT NOT NULL,
    updated_at     TEXT NOT NULL,
    archived_at    TEXT,
    UNIQUE(work_type_id, key)
);

CREATE TABLE work_attribute_options (
    id                     TEXT PRIMARY KEY,
    attribute_definition_id TEXT NOT NULL REFERENCES work_attribute_definitions(id) ON DELETE CASCADE,
    label          TEXT NOT NULL,
    value          TEXT NOT NULL,
    sort_order     INTEGER NOT NULL DEFAULT 0,
    is_active      INTEGER NOT NULL DEFAULT 1
);
CREATE INDEX idx_attr_options_def ON work_attribute_options(attribute_definition_id);

-- Tipli deger saklama: her veri tipi kendi kolonuna yazilir.
CREATE TABLE work_attribute_values (
    id                     TEXT PRIMARY KEY,
    work_item_id           TEXT NOT NULL REFERENCES work_items(id) ON DELETE CASCADE,
    attribute_definition_id TEXT NOT NULL REFERENCES work_attribute_definitions(id),
    value_text     TEXT,
    value_number   REAL,
    value_boolean  INTEGER,
    value_date     TEXT,
    value_datetime TEXT,
    value_json     TEXT, -- multiselect vb.
    updated_by     TEXT NOT NULL REFERENCES users(id),
    updated_at     TEXT NOT NULL,
    UNIQUE(work_item_id, attribute_definition_id)
);
CREATE INDEX idx_attr_values_item ON work_attribute_values(work_item_id);
CREATE INDEX idx_attr_values_def  ON work_attribute_values(attribute_definition_id);
