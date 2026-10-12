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
| F3-2 hava + hava savunma | yapılıyor | | |
| F3-3 ZMA, keşif | bekliyor | | |
| F3-4 çevre | bekliyor | | |
