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
| F1-2 yol ağı | bekliyor | | |
| F1-3 bölgeler | bekliyor | | |
| F1-4 ikmal | bekliyor | | |
| F1-5 radar/ESM/karıştırma | bitti | (bu commit) | 4 yeni birim tipi; kerteriz yalnızca yön verir; hile testi EW birimleriyle |
| F1-6 istemci | bekliyor | | |
| F1-7 kapı | bekliyor | | |
