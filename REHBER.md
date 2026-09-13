# Atölye Takip — Kullanım Rehberi

Bu rehber, platformu sıfırdan kurup günlük operasyonu yürütmek için adım adım hazırlanmıştır.
Örneklerde **15 katlı bir binada her daireye mutfak tezgahı ve tezgah arası işçiliği takip eden
bir atölye** senaryosu kullanılır; aynı adımlar fabrika, saha servis, yazılım ekibi gibi her yapıya uygulanır.

---

## İçindekiler

1. [Giriş ve Hesap](#1-giriş-ve-hesap)
2. [Workspace Oluşturma](#2-workspace-oluşturma)
3. [Ekip Kurma: Üyeler, Roller, Takımlar, Kapsam](#3-ekip-kurma)
4. [Yapıyı Kurma: Bölüm Ağacı](#4-yapıyı-kurma-bölüm-ağacı)
5. [İş Tipleri ve Özel Alanlar](#5-iş-tipleri-ve-özel-alanlar)
6. [Akış Tasarlama](#6-akış-taslama)
7. [İş Kalemleri Oluşturma](#7-iş-kalemleri-oluşturma)
8. [Süreçleri Yürütme](#8-süreçleri-yürütme)
9. [Onaylama ve Reddetme](#9-onaylama-ve-reddetme)
10. [Rework: Yeniden Üretim](#10-rework-yeniden-üretim)
11. [Yorumlar ve Dosyalar](#11-yorumlar-ve-dosyalar)
12. [Bildirimler](#12-bildirimler)
13. [Dashboard ve Raporlar](#13-dashboard-ve-raporlar)
14. [Günlük Kullanım Akışı](#14-günlük-kullanım-akışı)
15. [İpuçları ve En İyi Uygulamalar](#15-ipuçları-ve-en-iyi-uygulamalar)

---

## 1. Giriş ve Hesap

### Kayıt Olma
1. Uygulamayı açın → **Kayıt olun** bağlantısına dokunun
2. Ad Soyad, e-posta ve en az 8 karakter şifre girin
3. Kayıt biter bitmez otomatik giriş yapılır

### Giriş Yapma
- E-posta ve şifrenizle giriş yapın. Oturum 30 gün hatırlanır.

> **Not:** Kayıtlı olmayan kişi workspace'e eklenemez. Ekip üyelerinin önce kendi
> hesaplarını oluşturması gerekir.

---

## 2. Workspace Oluşturma

Workspace, tüm verilerinizin izole olduğu en üst sınırdır (şirket, atölye, proje).

1. Giriş sonrası **Workspaces** sayfasındasınız
2. **+ Yeni Workspace** → ad verin (örn. *Atölye Merkez*) → **Oluştur**
3. Siz otomatik olarak **Owner** (sahip) olursunuz

> Birden fazla workspace'e üye olabilirsiniz; sağ üstten **Çıkış** yaparak
> hesaptan çıkıp başka hesapla girebilirsiniz.

---

## 3. Ekip Kurma

**Ayarlar** sekmesinde dört bölüm: Üyeler, Roller, Takımlar. (Aktivite ve E-posta da buradadır.)

### 3.1 Üye Ekleme
1. **Ayarlar → Üyeler → + Ekle**
2. Üyenin **kayıtlı e-posta adresini** girin (henüz kayıt olmadıysa önce kayıt olmalı)
3. Rol seçin → **Ekle**

### 3.2 Roller
Varsayılan 5 sistem rolü:

| Rol | Ne yapabilir |
|---|---|
| **Owner** | Her şey; tek silinemez yönetici |
| **Admin** | Workspace'i arşivleme hariç her şey |
| **Supervisor** | Süreç onaylama, akış yayınlama, üye yönetimi hariç operasyon |
| **Worker** | Kendine atanan süreci başlatma/tamamlama |
| **Viewer** | Sadece görüntüleme |

- **Rol izinlerini düzenleyebilirsiniz**: Roller → düzenle → izinleri işaretleyin
- İhtiyaca göre **özel rol** oluşturabilirsiniz (örn. "Kalite Ekibi")

### 3.3 Kapsam (Scope) ile Erişim Sınırlama
Bir üyeyi sadece belirli bölümlere kısıtlamak için:
1. Üyeler → üyeye dokunun → **Erişim Kapsamı**
2. Görebileceği bölümleri seçin (alt bölümleri dahil olur)
3. Hiç seçilmezse → tüm workspace erişimi

> Örnek: *Ahmet sadece Blok A'yı görsün* → Blok A'yı işaretleyin.

### 3.4 Takımlar
1. **Takımlar → + Ekle** → ad (örn. *Montaj Ekibi*)
2. Takıma dokunun → üyeleri işaretleyerek ekleyin
3. Süreç atamalarında tek hamlede tüm takıma atayabilirsiniz

---

## 4. Yapıyı Kurma: Bölüm Ağacı

**Yapı** sekmesi, kart görünümünde seviye seviye inilen bir navigasyondur
(isterse sağ üstten **Ağaç** görünümüne geçilir).

### 4.1 Tek Bölüm Ekleme
Kart görünümlerinde **+ Bu seviyeye bölüm ekle** ya da bir kartın **⋮ menüsü → Alt Bölüm Ekle**.

### 4.2 Seri Oluşturma
Aynı seviyede çok sayıda bölümü tek seferde oluşturun:

- **⋮ menüsü → Seri Oluştur**
- Şablon: `Daire {nn}` → Başlangıç 1, Bitiş 10 → **Kaydet**
- Sonuç: *Daire 01 … Daire 10*

Tokenlar: `{n}` → 1,2,3 · `{nn}` → 01,02 · `{nnn}` → 001 · `{parent}` → üst bölüm adı

### 4.3 İç İçe Seri (Kat × Daire)
15 katlı bina, her katta 10 daire, daire numaraları 1'den 150'ye kadar:

1. **⋮ menüsü → Seri Oluştur → "İç içe seri"** seçeneğini açın
2. **Dış seviye:** `Kat {n}`, 1 – 15
3. **İç seviye:** `Daire {n}`, adet 10
4. **Numaralandırma:** *Devam eden* (1-10, 11-20, … 141-150) veya *Her seviyede baştan* (her katta 1-10)
5. **Kaydet** → 165 bölüm tek işlemde oluşur

### 4.4 Diğer İşlemler (⋮ menüsü)
- **İş Kalemi Ekle** → bu bölüme hızlı iş kalemi
- **Kopyala** → bölümü alt ağacıyla birlikte klonlar
- **Taşı** → başka bölümün altına taşır (döngü engellenir)
- **Arşivle** → bölümü ve altını geçici kapatır (veri kaybolmaz)

---

## 5. İş Tipleri ve Özel Alanlar

Her iş kalemi bir **tipe** bağlıdır (Tezgah, Boya, Bakım…). Tip tanımlarken özel alan şeması da tanımlanır.

### 5.1 Tip Oluşturma
**Ayarlar → İş Tipleri → + Ekle** → ad (örn. *Tezgah*) → **Oluştur**

### 5.2 Özel Alan (Özellik) Tanımlama
Tipin satırında **Özellikler** → **+ Yeni Özellik**:

| Alan | Açıklama |
|---|---|
| Özellik Adı | Örn. *Metraj* |
| Veri Tipi | Metin, Sayı, Tarih, Seçim, Çoklu Seçim, Para… (13 tip) |
| Zorunlu | İş kalemi kaydedilirken boş bırakılamaz |
| Filtrelenebilir | İşler listesinde filtre olarak kullanılır |
| Varsayılan Değer | Toplu oluşturmada otomatik dolar |

**Seçim** tiplerinde seçenekleri satır satır girin (Laminat / Granit / Mernet gibi).

> **Önemli kural:** Üzerinde veri oluşmuş bir özelliğin **veri tipi değiştirilemez**.
> Böyle bir durumda yeni özellik açıp eskisini arşivleyin.

### 5.3 Varsayılan Akış Bağlama
Tip düzenleme ekranında **Varsayılan Akış** seçerseniz, bu tipte iş kalemi oluşturulduğunda
akış **otomatik atanır** (bkz. Bölüm 6-7).

---

## 6. Akış Tasarlama

**Ayarlar → Akışlar** bölümünde süreç şablonları oluşturulur.

### 6.1 Yeni Akış
**+ Ekle** → ad (örn. *Tezgah Akısı*) → editör açılır.

### 6.2 Adım Ekleme ve Bağımlılıklar
1. **+ Adım Ekle** → ad (Kesim, İmalat, Sevkiyat…)
2. Her adım kartında **"Önce:"** satırına dokunun → hangi adım(lar) tamamlanınca
   bu adımın başlayabileceğini seçin
3. **Birden çok "önce" adımı** seçerek paralel dallar kurabilirsiniz:

```
Kesim → İmalat ─┐
                ├→ Montaj   (Montaj, İmalat VE Sevkiyat bitince başlar)
Kesim → Sevkiyat┘
```

> Döngü oluşturan bağlantılar (A→B→C→A) sistem tarafından reddedilir.

### 6.3 Adım Ayarları (adım kartına dokunun)
- **Onay Gerekir:** Tamamlanınca onay bekler (bkz. Bölüm 9). Onay Rolü seçebilirsiniz.
- **Varsayılan Atanan:** Bu adımı içeren her iş kaleminde otomatik olarak o kişiye/takıma
  atanır ve bildirim gider. *Kesim'i hep Ahmet yapıyorsa bir kez tanımlayın, bir daha
  uğraşmayın.*

### 6.4 Yayınlama
- **Yayınla** butonu → v1 yayınlanır → **yeni boş taslak (v2)** açılır
- Yayınlanan versiyon **asla değişmez**; düzeltme = yeni versiyon yayınlamak
- Halihazırda yürüyen işler eski versiyondan devam eder

> Farklı işler farklı akışlara bağlanabilir: *Tezgah Akısı* (Kesim→İmalat→Sevkiyat) ve
> *Tezgah Arası Akısı* (Kesim→Sevkiyat, İmalat'sız) gibi.

---

## 7. İş Kalemleri Oluşturma

### 7.1 Tek İş Kalemi
**İşler → + Yeni İş Kalemi** → bölüm, tip, ad → tipe özel alanlar dinamik çıkar → **Oluştur**.
Tipin varsayılan akışı varsa otomatik atanır.

### 7.2 Seri Oluşturma
**İşler → Seri** → bölüm + isim şablonu (`Tezgah {n}`, 1-10) → iş kalemi dizisi.

### 7.3 Bölümlere Dağıtma (güçlü özellik)
Her daireye otomatik iş kalemi açmak için:

1. **İşler → Bölümlere Dağıt**
2. **Kök Bölüm:** *Bina A* · **Hedef:** *En alt bölümler (daireler)*
3. **İş Tipi:** Tezgah · **İsim:** `Mutfak Tezgahı` (veya `{parent} Tezgah Arası`)
4. **Otomatik akış** açık kalsın → **Dağıt**

150 daireye 150 iş kalemi + akış ataması tek işlemde tamamlanır.

### 7.4 Listede Filtreleme
Üstteki segmentler: **Tümü / Bana Atanan / Onay Bekleyen** + Filtre (bölüm, tip, durum, arama).
Her kartta akış ilerleme yüzdesi gösterilir.

---

## 8. Süreçleri Yürütme

İş kalemi detayı → **Akış** sekmesi:

- Her adım bir karttır; durumlar: *Bekliyor → Hazır → Devam Ediyor → Tamamlandı*
- **Hazır** adımda **Başlat** → çalışma başlar (Deneme 1 kaydı oluşur)
- **Devam Ediyor** adımda **Tamamla** → adım biter
- Onay kuralı yoksa adım anında *Tamamlandı* olur ve **sonraki adımlar otomatik Hazır** yapılır
- Paralel dallar bağımsız ilerler; birleşme noktası tüm kollar bitince Hazır olur
- Tüm adımlar bitince akış **tamamlandı** olur

### Atamalar
- Karttaki **+ ata** ile kullanıcı veya takım atayın (bildirim giderler)
- **💬** simgesi ile adım hakkında yorum yapın

---

## 9. Onaylama ve Reddetme

Onay kuralı tanımlı adımlarda **Tamamla** → *Onay Bekliyor* durumuna geçer:

- Yetkili (atanan onay rolü / Owner / Admin) kartta **Onayla / Reddet** butonlarını görür
- **Reddet** için zorunlu sebep girilir → adım *Başarısız* olur → Rework başlatılabilir
- Onay geçmişi kartta görünür ("… tarafından onaylandı / reddedildi — gerekçe")

---

## 10. Rework: Yeniden Üretim

Başarısız adım **asla geri alınmaz**; bunun yerine yeni bir deneme zinciri başlatılır:

1. Akış başlığında **Rework** butonu görünür (başarısız adım varsa)
2. **Yeniden başlanacak adımı seçin** (örn. *Kesim* — hatanın kaynağı)
3. Zorunlu **sebep** girin → **Başlat**
4. Seçilen adım ve sonraki adımlar yeni deneme ile *Hazır/Bekliyor* olur;
   önceki onaylı adımlar **dokunulmaz kalır**
5. Kartlarda **"Deneme 2, 3…"** rozetleri ve altta **Rework Geçmişi** görünür

---

## 11. Yorumlar ve Dosyalar

İş kalemi detayında **Yorumlar** ve **Dosyalar** sekmeleri:

- **Yorumlar:** alttaki kutuya yazın → Gönder. Kendi yorumunuzu düzenleyebilirsiniz.
  Adım kartlarındaki 💬 ile **süreç bazlı** yorum da yapılır.
- **Dosyalar:** **+ Dosya Ekle** ile fotoğraf/PDF yükleme (10 MB'a kadar).
  Dosyalar bölüme/iş kalemine eklenir, indirilebilir, silinebilir (arşivlenir).

---

## 12. Bildirimler

Sağ üstteki **zil** simgesi:

- Okunmamış sayısı kırmızı rozette
- Olaylar: *Yeni görev ataması, Onay bekliyor, Onaylandı, Reddedildi, Rework başlatıldı*
- Bildirime dokununca okundu sayılır; **Tümünü okundu işaretle** de var
- Aynı bildirimler e-posta kuyruğuna da düşer (**Ayarlar → E-posta**'dan izlenebilir)

---

## 13. Dashboard ve Raporlar

**Genel Bakış** sekmesi:

- **8 metrik kartı:** aktif/tamamlanan/geciken iş, hazır/devam eden/onay bekleyen süreç, rework
- **Kalite Özeti:** İlk Seferde Başarı %, Ortalama Cycle Time, Rework Oranı
- **En Çok Hata Çıkan Süreçler** ve **Süreç Süreleri** grafikleri

**Ayarlar → Aktivite**: kim ne yaptı (append-only, değiştirilemez denetim kaydı).

---

## 14. Günlük Kullanım Akışı

### Kurulum (bir kez)
```
Workspace aç → Üyeleri davet et → Rollerini ver →
Yapıyı kur (iç içe seri) → İş tipleri + özel alanlar → Akışları tasarla ve yayınla →
Tiplere varsayılan akış bağla → Adımlara varsayılan atanan tanımla
```

### Operasyon (her gün)
```
İşler → Bana Atanan → süreci Başlat → Tamamla
   ↓ (onay gerekliyse)
Yetkili: Onayla / Reddet
   ↓ (reddedildiyse)
Rework başlat → döngü devam
```

### Yeni bina/site geldiğinde
```
Bölüm oluştur → İç içe seri (kat × daire) →
İşler → Bölümlere Dağıt (her daireye tezgah) → tekrar (tezgah arası) →
hepsi akışlarıyla hazır
```

---

## 15. İpuçları ve En İyi Uygulamalar

1. **Akışları önce yayınlayın**, sonra dağıtım yapın — otomatik atama yayınlanmış akış ister.
2. **Varsayılan atanan** kullanın: "Kesim'i hep Ahmet yapıyor" derdi biter; nadiren
   değişirse karttan düzenlersiniz.
3. **Kapsam (scope)** ile saha personelinin karışıklığını önleyin — herkes sadece
   kendi blokunu görür.
4. **İsim şablonlarında `{parent}`** kullanın: *Daire 3 Tezgah Arası* gibi kendini
   tanımlayan isimler oluşur.
5. **Onay kuralını** sadece gerçekten kontrol gereken adımlara koyun — gereksiz onay
   akışı yavaşlatır.
6. **Rework'te sebep** yazmayı alışkanlık edinin; "En Çok Hata Çıkan Süreçler" raporu
   bunlardan beslenir.
7. Yayınlanmış akışta hata bulursanız panik yok: yeni versiyonu düzeltip yayınlayın,
   yürüyen işler eski versiyondan zarar görmeden tamamlanır.

---

*Sorular, ek özellik istekleri ve hata bildirimleri için geliştiriciye ulaşın.*
