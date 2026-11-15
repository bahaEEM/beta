# İlerleme — Faz 0

| Görev | Durum | Commit | Not |
|---|---|---|---|
| G-ENV | bitti | – | Rust 1.99, cargo-deny, Godot 4.6.3 (~/.local/bin/godot). GDAL/mold/perf apt ile kurulmadı; harita için rasterio venv'i (veri/venv-rio) kullanıldı |
| G0 | bitti | (bu commit) | mold yok → .cargo/config.toml sonra |
| G1 | bitti | (bu commit) | units.toml + damage_hp, World, HeightMap |
| G2 | bitti | 29e1c87 | alt ajan |
| G3 | bitti | f50ad39 | alt ajan; ölü birim görünmez olur |
| G4 | bitti | bbdc8a0 | atış için atıcının kendi görüş hattı da şart |
| G5 | bitti | (bu commit) | iz 1 = 500 m hücre merkezi; kayıp iz hep bölge |
| G6 | bitti | (bu commit) | G5 olaylarına göre küçük ayar gerekebilir; docs/protocol.md testten üretilir |
| G7 | bitti | (bu commit) | 5 uçtan uca test; data/scenarios/hasandag.toml |
| G8 | bitti | 635029a + (bu commit) | gerçek harita rasterio ile üretildi; 954–3262 m |
| G9 | bitti (görsel test kullanıcıda) | dbb6dfe | ekransız smoke + arazi testi OK; FPS ~144 |
| G10 | bitti | (bu commit) | docs/faz0-rapor.md |

## Açık sorular
- (yok)

# Faz 1

| Görev | Durum | Commit | Not |
|---|---|---|---|
| F1-0 ölçek | bitti | (bu commit) | 2 dk/tur; Hasandağı 5 turda 8 atış, 7 imha |
| F1-1 grup emirleri + KBG | bitti | (bu commit) | protokol v2: move{units}, stop, orders_accepted; istemci arayüzü F1-6 |
| F1-2 yol ağı | bitti | f61505d + (bu commit) | OSM (ODbL) 36 bin yol hücresi; A* yolu tercih eder, yolda speed_road_mps |
| F1-3 bölgeler | bitti | (bu commit) | 5 bölge (2 depo); 60 sn piyade varlığı; tur sonu puan; 100 puan / 100 tur zafer |
| F1-4 ikmal | bitti | (bu commit) | yakıt/mühimmat; ikmal kamyonu 300 m, depoda dolar |
| F1-5 radar/ESM/karıştırma | bitti | (bu commit) | 4 yeni birim tipi; kerteriz yalnızca yön verir; hile testi EW birimleriyle |
| F1-6 istemci | bitti (görsel test kullanıcıda) | 706c1d8 | alt ajan; ekransız testler OK, ~130 FPS |
| F1-7 kapı | bitti | (bu commit) | G10 testi grup/EW emirleriyle; docs/faz1-rapor.md |

# Faz 2

| Görev | Durum | Commit | Not |
|---|---|---|---|
| F2-1 muharebe modeli + denge + design.md | bitti | (bu commit) | beklenen hasar, nişan, ateş kuralı (set_stance), tanksavar timi, senaryo 5,6 km; istemcide ateş kuralı tuşu F2 sonunda |
| F2-2 haberleşme ağı | bitti | (bu commit) | karargâh, telsiz röle, yol kablosu, karıştırıcı telsizi keser, kopuk birime emir yok |
| F2-3 topçu ve füze | bitti | (bu commit) | obüs 3–30 km, füze rampası 10–70 km; nokta/birim atışı; uçuş süresi; alan hasarı; G10'da tur başı 12 mermi |
| F2-4 istemci (ateş kuralı, atış görevi, bağlantı durumu, patlamalar) | bitti | ddf6c08 | alt ajan; ekransız testler OK; docs/faz2-rapor.md |

# Faz 3

| Görev | Durum | Commit | Not |
|---|---|---|---|
| F3-0 senaryo kuzey/güney + 60 sn | bitti | 24f9cff | ilk temas 11. turda → F3-2 ile hızlanacak |
| F3-1 istemci görselleri | yapılıyor | | alt ajan |
| F3-2 hava + hava savunma | bitti | (bu commit) | İHA, SİHA, hava savunma; ilk çatışma 11. → 3. tur |
| F3-3 ZMA, keşif | bitti | (bu commit) | senaryoda taraf başı 2 ZMA + 1 keşif |
| F3-4 çevre | bekliyor | | |

# Mekanik tamamlama (fazlar kaldırıldı, 2026-10-12)

Plan: docs/MEKANIK_PLAN.md (M1 harita/bölgeler/deniz → M2 yapılar/sağlık → M3 fiziksel ikmal → M4 gizlenme/pusu/baskı →
M5 EH derinliği → M6 deniz/sonar → M7 ekonomi/takviye). Görsel giydirme en sona.
Önceden biten: F3-1 görseller (11cb52a), helikopter (34acb21), ZMA'ya binme/inme (31083b8).

| Paket | Durum | Commit | Not |
|---|---|---|---|
| M1a deniz + arazi örtüsü | bitti | (bu commit) | doğu denize çevrildi (kıyı x≈24–31 km, %34 su); terrain.bin: açık/orman/yerleşim/su; kara birimi suya giremez (K-024/K-025); istemci denizi mavi boyar. Orman/yerleşim istemcide henüz çizilmiyor (M4'te) |
| M1b adlı bölgeler | bitti (görsel test kullanıcıda) | (bu commit) | 30 bölge (regions.toml); üsler Helvadere (Mavi) / Karakapı (Kırmızı), 8+8 sahipli, 14 tarafsız; puan kalktı: düşman üssünü alan kazanır, 100. turda çok bölgesi olan (K-061, K-063); protokol v3; istemci sınır + cephe hattı çizer. Birimler yeni üslere kaydırıldı |
| M2 yapılar + kurulum + sağlık | bitti (görsel test kullanıcıda) | (bu commit) | K-120 otomatik kurulum (obüs, füze, radarlar, ESM, karıştırıcı, MG ve roketatar timleri); 8 yapı tipi, bölgeyle el değiştirir (K-121/122); sağlık aracı + sahra hastanesi (K-123); yeni birimler: mg_team, rocket_inf, radar_mobile, ambulance; protokol v4. Lojistik yapılar M3'e kadar işlevsiz; yapı modelleri geçici kutu |
| M3 fiziksel ikmal | bitti (görsel test kullanıcıda) | (bu commit) | K-130…K-134: yol bağlantılı bölge ağı, stoklu yapılar, lojistik üs üretimi, otomatik dolan/boşaltan kamyon ve kargo uçağı, bölge ikmali (%25/tur), 3 tur ikmalsiz kalan kara birimi erir. Bölge `depot` bayrağı kalktı. Protokol v5 (`supplied`, `unsupplied_turns`). İstemci: binme (B) / inme (N) tuşları. İkmal gemisi M6'ya kaldı |
| M4 gizlenme/pusu/baskı/aldatma | bitti (görsel test kullanıcıda) | (bu commit) | K-140 gizlenme (orman, yerleşim, piyade, durma; ateş ifşa eder), K-141 piyadeye koruma, K-142 Pusu ateş kuralı (yarı menzil, görülmeden vurana 1,5 hasar), K-143 baskı (isabet/hız düşer, piyade yere yatar), K-144 sahte tank + sahte radar. Protokol v6. İstemci orman/yerleşimi boyar. Sayılar tahmini, denge ayarı yok |
| M5 EH derinliği | bitti (görsel test kullanıcıda) | (bu commit) | K-150 emir alan birim telsiz yayını yapar (ESM duyar; kablodaki yapmaz), K-151 telsiz sessizliği (X tuşu; yeni emir almaz, röle olmaz), K-152 kamikaze İHA rampası (yayına kilitlenir), K-153 karşı-batarya radarı, K-154 "Karşılık"taki topçu kendiliğinden karşı atış yapar, K-155 bağı kopan İHA eve döner. Protokol v7. Röle yayını modellenmedi |
| M6 deniz + sonar | bitti (görsel test kullanıcıda) | (bu commit) | K-160…K-165: devriye botu, korvet (kara bombardımanı), denizaltı, çıkarma gemisi, ikmal gemisi, kıyı gemisavar bataryası; aktif/pasif sonar; her kıyıya çıkarma. İstemci ekransız testleri (smoke, main_check, terrain_check) yeşil; gemi modelleri geçici kutu |
| M7 ekonomi + takviye | bitti (görsel test kullanıcıda) | (bu commit) | K-170…K-173: ikmalli bölge başına gelir (20/tur, başlangıç 300), `buy` emri, üretim kuyruğu (kara: lojistik üs, hava: hava alanı, deniz: liman), yapı düşerse sipariş yanar. Protokol v8 (`money`, `income`, `production`, `shop`). İstemci: üst satırda para, yapı seçiliyken U ile satın alma menüsü, Delete ile vazgeçme. İstemcide `own_units` alanlarının (sessizlik, kurulum, araçta, baskı…) birim kaydına kopyalanmaması düzeltildi. Fiyatlar tahmini, denge raporu yok; onarım ve inşaat M8'e alındı |
| M8a istihkâm: onarım + inşaat | bitti (görsel test kullanıcıda) | (bu commit) | K-180 onarım (araç + yapı, tur başı %20, paralı), K-181 inşaat (`build` emri; onarım çadırı, iyileştirme çadırı, baz istasyonu kulesi, sığınak, hava savunma mevzisi; istihkâm başında bekler), K-182 sığınak 2 piyade timini korur. Yeni birim: istihkâm aracı (250 para; senaryoda taraf başı 1). Protokol v9. İstemci: istihkâm seçiliyken U ile inşaat menüsü. Yeni yapıların modeli geçici kutu; sayılar tahmini |
| M8b mayın + hendek | bitti (görsel test kullanıcıda) | (bu commit) | K-185 tanksavar mayın tarlası (düşman aracına hasar + durdurur, 4 patlama), K-186 gizlilik (piyade / keşif / istihkâm 200 m'den bulur), K-187 hendek (araç geçemez, rota dolanır), K-188 `clear` emriyle temizlik. Protokol v10 (`lay`, `clear`, `obstacles`). İstemci: istihkâm seçiliyken M mayın, H hendek (sonra yön için tık), K temizle; tarlalar daire, hendekler çizgi. Sayılar tahmini |
| Denge 1. tur | bitti | (bu commit) | Eşit paralı düello raporu (`balance duels`, docs/denge-rapor.md). K-047 tanksavar füzesi piyadeye atılmaz; tank menzili 2500 m, roketatar 700 m; fiyatlar: piyade 80, MG 150, roketatar 120, tanksavar 220, keşif 150, ZMA 280, tank 550. Topçu, deniz, EH ve ekonomi hızı ölçülmedi |
