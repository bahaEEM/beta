# Faz 1 raporu (2026-10-07)

Hedef: kusursuz mekanik ve altyapı — ölçek, grup emirleri, yol/bölge/ikmal, radar/ESM/karıştırma, oynanabilir istemci.
Tasarım belgesi (K-kimlikleri) ve görsel efektler Faz 2'de.

## Yapılanlar
| Görev | Ne | Commit |
|---|---|---|
| F1-0 ölçek | 1 tur = 2 dk oyun süresi (`turn.game_seconds_per_tick = 0.6`); yeniden dolum saniye cinsinden | b449f42 |
| F1-1 grup emirleri | `move {units}` (düzen korunur), `stop`, KBG = her başlayan 12 birim için 1 puan; `orders_accepted` | 5aa2b46 |
| F1-2 yol ağı | OSM'den 2838 yol çizgisi, 36 bin yol hücresi (3 sınıf); A* yolu tercih eder, yolda `speed_road_mps` | f61505d, a930296 |
| F1-3 bölgeler | 5 bölge (2 depo + 3 hedef); sadece piyade ele geçirir (60 sn tek başına); tur sonu puan; 100 puan veya 100 turda zafer, `game_over` | a930296 |
| F1-4 ikmal | yakıt (km başına) ve mühimmat (atış başına); ikmal kamyonu tur sonunda 300 m içini doldurur, kendi depo bölgesinde dolar | a930296 |
| F1-5 elektronik harp | kara gözetleme radarı (açıkken, hareketli hedef, iz 2), ESM (yayın yapanın yönü = iz 1 kerteriz; iki kerteriz ≥20° kesişirse iz 2), karıştırıcı (açıkken kapsadığı düşman radarının izini 1 düşürür); `set_emitter` emri | 78f1e91 |
| Kayıt | harita artık kayda girmiyor: tur kaydı 2,7 MB → ~0,9 KB; `--resume` haritayı yeniden yükler | a9270e6 |
| F1-6 istemci | kutu seçimi, Ctrl+1–9 grupları, grup rota, emir silme (Delete), dur (S), radar/karıştırıcı aç-kapa (R), bilgi paneli (hp/yakıt/mühimmat/yayın), yollar, bölgeler, skor, oyun sonu paneli, kerteriz çizgileri | 706c1d8 |

## Ölçümler (Hasandağı, taraf başına 12 birim, release)
| Tur | Çözüm süresi | Olay | Atış | Toplam imha |
|---|---|---|---|---|
| 1 | 15 ms | 4557 | 5 | 4 |
| 2 | 10 ms | 2897 | 11 | 15 |
| 3–5 | ~2 ms | ~1700 | 0 | 15 |

- 3 ağ koşusu bayt düzeyinde aynı; hile testi (20 rastgele senaryo EW birimleriyle + Hasandağı + ağ) temiz:
  yalnızca kerterizle bilinen birimin konumu hiçbir mesajda yok.
- 72 Rust testi + Godot ekransız testleri (smoke, main_check, terrain_check) yeşil; clippy, cargo deny temiz.
- İstemci: yollar ve bölgelerle ~130 FPS (RTX 3050; Faz 0'da 144, ekran sınırı).

## Kontroller
Sol tık seç · sol sürükle kutu seç · Shift+tık ekle/çıkar · Ctrl+1–9 grup kaydet, 1–9 çağır · sağ tık rota noktası ·
Delete emirleri sil · S dur · R radar/karıştırıcı aç-kapa · WASD/oklar kaydır · Q/E döndür · tekerlek yakınlaştır.

## Bilinen eksikler / kararınızı bekleyenler
- **Denge:** sayılar yer tutucu. 2 dk'lık turda ilk iki turda 24 birimden 15'i yok oluyor — tanklar çok ölümcül
  ya da başlangıç çok yakın. Faz 2 dengesinde bakılmalı.
- Bölge ele geçirme herkese açık (bayrak); bu, o bölgede düşman piyadesi olduğunu ele verir — bilinçli seçim.
- Yol ağı sadece hareket hızı için; "kablolu haberleşme ağı" ve yola bağlı lojistik henüz yok.
- Radar sadece hareket edeni görür (duran hedefi görmez); karıştırma sadece radarı etkiler (ESM'i değil).
- `*_link` yolları (kavşak bağlantıları) OSM sorgusunda yok.
- Görsel test sizde: iki pencereyle bir maç oynanmalı. Kerteriz/bölge çizimleri ve oyun sonu paneli ekranda görülmedi (sadece ekransız test).
- İlk açılışta (yeni kopyada) bir kez `godot --headless --path client --import` gerekebilir.

## Faz 2 önerileri
1. docs/design.md + K-kimlikleri; denge tablosu (menzil, hız, zırh, sensör).
2. Görsel: birim modelleri, ateş/patlama efektleri, arazi dokusu, hava durumu (ilgili kuralları sonra).
3. Haberleşme ağı (kablolu yol hattı + telsiz), karıştırmanın haberleşmeye etkisi; KBG'nin haberleşmeye bağlanması.
4. Topçu/füze (dolaylı ateş, iz 2 ile), hava birimleri.
