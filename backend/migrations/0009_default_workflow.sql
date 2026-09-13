-- Gercek senaryo paketi: is tipine varsayilan akis baglama.
-- Tezgah -> Tezgah Akisi, Tezgah Arasi -> Tezgah Arasi Akisi gibi;
-- is kalemi olusturulurken otomatik atama icin kullanilir.

ALTER TABLE work_types ADD COLUMN default_workflow_template_id TEXT REFERENCES workflow_templates(id);
