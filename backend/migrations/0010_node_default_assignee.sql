-- Adima varsayilan atanan: "Kesim'i hep Ahmet yapar" bir kez tanimlanir,
-- her instance'da otomatik atanir + bildirim gider.

ALTER TABLE workflow_nodes ADD COLUMN default_assignee_type TEXT; -- user | team (null = yok)
ALTER TABLE workflow_nodes ADD COLUMN default_assignee_id TEXT;
