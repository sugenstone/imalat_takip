/// Sistemdeki tum izinler (yol haritasi 7.2).
/// Migration seed ile birebir uyusmak zorunda.
pub const ALL_PERMISSIONS: &[(&str, &str)] = &[
    ("workspace.update", "Workspace bilgilerini guncelleme"),
    ("workspace.archive", "Workspace arsivleme"),
    ("section.create", "Bolum olusturma"),
    ("section.update", "Bolum guncelleme"),
    ("section.clone", "Bolum kopyalama"),
    ("section.archive", "Bolum arsivleme"),
    ("section.bulk_create", "Seri bolum olusturma"),
    ("work_item.create", "Is kalemi olusturma"),
    ("work_item.update", "Is kalemi guncelleme"),
    ("work_item.archive", "Is kalemi arsivleme"),
    ("work_item.assign_workflow", "Is kalemine workflow atama"),
    ("workflow.create", "Workflow sablonu olusturma"),
    ("workflow.update", "Workflow sablonu guncelleme"),
    ("workflow.publish", "Workflow versiyonu yayinlama"),
    ("workflow.assign", "Workflow atama"),
    ("process.start", "Surec baslatma"),
    ("process.complete", "Surec tamamlama"),
    ("process.fail", "Surec basarisiz isaretleme"),
    ("process.rework", "Rework baslatma"),
    ("process.approve", "Surec onaylama"),
    ("process.assign", "Surec atama"),
    ("team.manage", "Takim yonetimi"),
    ("user.invite", "Uye ekleme/cikarma"),
    ("role.manage", "Rol ve kapsam yonetimi"),
    ("notification.manage", "Bildirim yonetimi"),
];

/// Workspace olusturulurken yaratilan varsayilan sistem rolleri.
/// Owner sabittir; digerlerinin izinleri role.manage ile duzenlenebilir.
pub struct SystemRoleDef {
    pub name: &'static str,
    pub description: &'static str,
    pub permissions: &'static [&'static str],
}

const OWNER_PERMS: &[&str] = &[
    "workspace.update",
    "workspace.archive",
    "section.create",
    "section.update",
    "section.clone",
    "section.archive",
    "section.bulk_create",
    "work_item.create",
    "work_item.update",
    "work_item.archive",
    "work_item.assign_workflow",
    "workflow.create",
    "workflow.update",
    "workflow.publish",
    "workflow.assign",
    "process.start",
    "process.complete",
    "process.fail",
    "process.rework",
    "process.approve",
    "process.assign",
    "team.manage",
    "user.invite",
    "role.manage",
    "notification.manage",
];

const ADMIN_PERMS: &[&str] = &[
    "workspace.update",
    "section.create",
    "section.update",
    "section.clone",
    "section.archive",
    "section.bulk_create",
    "work_item.create",
    "work_item.update",
    "work_item.archive",
    "work_item.assign_workflow",
    "workflow.create",
    "workflow.update",
    "workflow.publish",
    "workflow.assign",
    "process.start",
    "process.complete",
    "process.fail",
    "process.rework",
    "process.approve",
    "process.assign",
    "team.manage",
    "user.invite",
    "notification.manage",
];

const SUPERVISOR_PERMS: &[&str] = &[
    "section.create",
    "section.update",
    "section.clone",
    "section.bulk_create",
    "work_item.create",
    "work_item.update",
    "work_item.archive",
    "work_item.assign_workflow",
    "workflow.assign",
    "process.start",
    "process.complete",
    "process.fail",
    "process.rework",
    "process.approve",
    "process.assign",
    "team.manage",
    "notification.manage",
];

const WORKER_PERMS: &[&str] = &["process.start", "process.complete", "work_item.update"];

pub const SYSTEM_ROLES: &[SystemRoleDef] = &[
    SystemRoleDef {
        name: "Owner",
        description: "Workspace sahibi - tum yetkiler",
        permissions: OWNER_PERMS,
    },
    SystemRoleDef {
        name: "Admin",
        description: "Yonetici - workspace arsivleme haric tum yetkiler",
        permissions: ADMIN_PERMS,
    },
    SystemRoleDef {
        name: "Supervisor",
        description: "Saha sorumlusu - operasyonel yetkiler",
        permissions: SUPERVISOR_PERMS,
    },
    SystemRoleDef {
        name: "Worker",
        description: "Calisan - surec baslatma/tamamlama",
        permissions: WORKER_PERMS,
    },
    SystemRoleDef {
        name: "Viewer",
        description: "Gozlemci - sadece goruntuleme",
        permissions: &[],
    },
];

pub fn is_valid_permission(key: &str) -> bool {
    ALL_PERMISSIONS.iter().any(|(k, _)| *k == key)
}
