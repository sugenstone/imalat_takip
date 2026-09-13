# İmalat Takip — Operasyon Takip Platformu

Dinamik workspace, iş kalemi ve workflow takip platformu. Yol haritasındaki
**tüm MVP fazları tamamlandı**:

| Faz | İçerik | Durum |
|---|---|---|
| 1 | Auth + Workspace + Yetki (RBAC + section scope) | ✓ |
| 2 | Section engine (sınırsız ağaç, seri, klon, taşıma) | ✓ |
| 3 | İş kalemleri + dinamik özellikler (12 veri tipi) | ✓ |
| 4 | Workflow designer (template, versiyon, DAG, publish) | ✓ |
| 5 | Workflow runtime (instance, süreç motoru, cascade) | ✓ |
| 6 | Atama (user/team) + onay katmanı (submit ≠ approve) | ✓ |
| 7 | Rework / attempt zinciri (geçmiş asla ezilmez) | ✓ |
| 8 | Yorum + dosya + audit (append-only aktivite) | ✓ |
| 9 | Bildirim + e-posta (event tabanlı, outbox kuyruk) | ✓ |
| 10 | Dashboard + raporlar (cycle time, FP oranı, hata analizi) | ✓ |

## Teknoloji

| Katman    | Teknoloji                                      |
| --------- | ---------------------------------------------- |
| Backend   | Rust (Axum + SQLx)                              |
| Frontend  | SvelteKit 2 (Svelte 5) + Tailwind CSS v4        |
| Veritabanı | SQLite (PostgreSQL geçişi için hazır)          |
| Auth      | Argon2 + httpOnly cookie session                |
| Mobil     | Tüm UI mobile-first (alt tab bar, bottom-sheet) |

## Geliştirme Ortamı

**Backend** (http://localhost:8080):

```bash
cd backend
cargo run
```

İlk çalıştırmada `data/app.db` otomatik oluşur, migration'lar uygulanır.

**Frontend** (http://localhost:5173):

```bash
cd frontend
npm install
npm run dev
```

Vite, `/api` isteklerini backend'e proxy'ler — cookie'ler aynı origin'de sorunsuz çalışır.

## Production (tek binary)

```bash
cd frontend && npm run build          # -> frontend/build
cd backend
IMTK_STATIC_DIR=../frontend/build cargo run --release
```

Rust binary hem API'yi hem SPA'yı tek porttan servis eder. SQLite dosyası
yanında taşınabilir.

## Ortam Değişkenleri (backend)

| Değişken           | Varsayılan                        | Açıklama                      |
| ------------------ | --------------------------------- | ----------------------------- |
| `IMTK_HOST`        | `0.0.0.0`                         | Dinleme adresi                |
| `IMTK_PORT`        | `8080`                            | Port                          |
| `IMTK_DB_URL`      | `sqlite://data/app.db?mode=rwc`   | Veritabanı                    |
| `IMTK_STATIC_DIR`  | _(boş)_                           | SvelteKit build klasörü       |
| `IMTK_COOKIE_SECURE` | `false`                         | HTTPS arkasında `true` yapın  |
| `IMTK_UPLOAD_DIR`  | `data/uploads`                    | Dosya ekleri kök klasörü      |

## Testler

```bash
cd backend && cargo test    # 12 test paketi, 27 uçtan uca API testi
cd frontend && npm run check # tip kontrolü
```

## Mimari Notlar

- **UUIDv7** ID'ler (TEXT) — PostgreSQL geçişinde UUID tipine
- **RFC3339 UTC** tarihler — PostgreSQL'de TIMESTAMPTZ
- Materialized path (`path_cache`) ile sınırsız derinlikte bölüm ağacı
- Kapsam (scope): üyeye section subtree erişimi verilebilir; kayıt yoksa tam erişim — tüm listeleme/raporlar scope duyarlı
- Hard delete yok — archive/soft-delete politikası
- Published workflow versiyonları immutable; her değişiklik yeni versiyon
- Süreçler DAG bağımlılıklarıyla ilerler; cycle kontrolü hem eklemede hem publish'te
- Onay kuralı node bazında (onay rolü kısıtıyla); submit ≠ approve
- Rework'te geri alma yok: yeni attempt zinciri, eskiler superseded — geçmiş korunur
- Audit log append-only; bildirimler event tabanlı; e-postalar outbox kuyruğundan işlenir
- Dashboard: aktif/tamamlanan/geciken iş, süreç durumları, rework sayısı, ortalama cycle time, ilk seferde başarı oranı, en çok hata çıkan süreçler, süreç süreleri

## PostgreSQL Geçişi Notları

- Migration'lar standart SQL ile yazıldı; geçişte dialect uyarlaması:
  `TEXT` id → `UUID`, tarihler → `TIMESTAMPTZ`, `julianday()` farkları →
  epoch hesaplarına, `json_group_array` → `json_agg`
- SQLx her iki veritabanını destekler; `IMTK_DB_URL` değişikliği yeterli olacaktır
