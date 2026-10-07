# Faz 0 görevleri

Hedef: Hasandağı haritasında, 2 birim türüyle (piyade, ana muharebe tankı) LAN üzerinden uçtan uca WEGO turu.
Sıra: G-ENV -> G0 -> G1 -> {G2, G3, G8 paralel} -> G4 -> G5 -> G6 -> G7 -> G9 -> G10. G9; G6, G7, G8 bitmeden başlamaz.
Doğrulanan her görev = tek commit ("G3: line of sight and track quality"), sonra PROGRESS.md güncellenir.

## Proje özeti
- Deterministik, zarsız, eşzamanlı turlu (WEGO) elektronik harp oyunu. 3B, C&C Generals görsel dili. Sadece çok oyunculu.
- Tur: planlama 45–60 sn, çözüm 20 sn (200 adım), tur sonu ekonomi. Maç 70–100 tur.
- İz kalitesi 0–3 (0 yok, 1 temas/bölge, 2 konum+sınıf, 3 atış kalitesi). Aktif sensörler duyulur; pasif ESM yön verir; üçgenleme; karıştırma iz seviyesini düşürür.
- KBG: tur başı emir bütçesi. İleride kontrol grupları, ikmal, bölge ele geçirme (sadece piyade), yol ağı.
- İleride 49 birim (kara/hava/deniz), sonar, füze, 40–50 km haritalar → her şey veri odaklı (TOML), çoklu sensör tipi, oyuncu başına görünürlük.
- Yetkili sunucu; istemci görmediği birimin verisini hiç almaz. Mesajlar JSON + zstd. Sunucu her tur sonu diske kaydeder.
- Hedef makine: Debian, Ryzen 5 7000, 16 GB, RTX 3050 Laptop 4 GB.

## Verilmiş kararlar (2026-10-07)
- docs/design.md yok; görev metinleri belirleyici. Test adları görev kimliğiyle (g4_...).
- Piyade hasarı: units.toml'a `damage_hp` alanı eklenir (atış başına hp kaybı). Piyade zırhı 0 → zırhlı hedef hesabı piyadeye uygulanmaz, hp düşer.
- "Hasarlı" birim tekrar hasarlı sonucu alırsa imha olur.
- Alt ajanlar sadece paralel görevlerde (G2, G3, G8); diğerleri ana oturumda.
- Yükseklik verisi: Copernicus GLO-30 (AWS açık bucket, hesap gerekmez).

## G-ENV — Ortam
rustup/cargo, git, cargo-deny, mold, GDAL, Godot 4, nvidia-smi, vulkaninfo kontrolü; sudo gerekenler kullanıcıya.

## G0 — Repo iskeleti
- Cargo workspace: sim (lib), protocol (lib), server (bin), tools (bin). client/ altında sadece README.
- data/, maps/, saves/, docs/; saves/ .gitignore'da.
- LICENSE (AGPL-3.0 tam metin), LICENSE-ASSETS (CC BY-SA 4.0), ASSETS.md (dosya, kaynak, araç, lisans, tarih).
- deny.toml: MIT, Apache-2.0, BSD-2-Clause, BSD-3-Clause, ISC, Zlib, MPL-2.0, Unicode-3.0, Unicode-DFS-2016.
- .github/workflows/ci.yml: fmt, clippy -D warnings, test, cargo-deny. mold kuruluysa .cargo/config.toml.
Kabul: cargo build --all, cargo test --all, cargo deny check.

## G1 — Veri modeli
- data/units.toml: infantry, mbt. Alanlar: id, name_tr, class (infantry|vehicle), speed_offroad_mps, max_slope_deg,
  armor_front_mm, armor_side_mm, armor_rear_mm, weapon_range_m, penetration_mm, reload_ticks, sensor_optical_range_m, hp
  (+ damage_hp). Menzil: piyade 400 m, tank 2000 m. Diğerleri yer tutucu, yanına # placeholder.
- sim::data: TOML serde ile okunur ve doğrulanır (negatif değer, eksik alan -> hata).
- sim::world: World { tick, units: Vec<Unit>, map: HeightMap }; Unit { id: UnitId, owner: PlayerId, type_id, pos (m, f64), heading_rad, hp, state }.
- UnitId, PlayerId newtype; birimler id sırasıyla.
- HeightMap: genişlik, yükseklik, hücre boyutu (m), Vec<f32>; height_at(pos) bilinear.
Kabul: geçerli/hatalı TOML, height_at, sıralama testleri.

## G2 — Hareket ve yol bulma
- MoveOrder { unit, waypoints: Vec<Vec2> }.
- Izgara A* (pathfinding crate); max_slope_deg üstü hücre geçilmez; maliyet mesafe/hız; eşit maliyette sabit komşu sırası.
- Her tick rotada hız x 0,1 s ilerler. Yol verisi yok; çarpışma yok.
Kabul: düz zeminde en kısa yol; dik yamacı dolaşma; aynı emirlerle 200 tick iki kez → konumlar bit düzeyinde aynı.

## G3 — Görüş hattı ve iz kalitesi
- line_of_sight(map, from, to, eye_height_m, target_height_m): ışın örnekleme, adım = hücre boyutunun yarısı.
- Optik: menzil içi + görüş açık → 3; 1,5x menzil içi + görüş açık → 1; aksi 0. Sensör sistemi radar/ESM/sonar eklenebilir olsun.
- Track { observer_player, target_unit, quality: u8, last_seen_tick, last_known_pos }; her tick her oyuncu için; kayıpta last_known_pos saklanır.
Kabul: tepe arkası görünmez, menzil sınırları, son bilinen konum.

## G4 — Ateş ve hasar
- Hedef: iz 3 + menzildeki en yakın düşman; eşitlikte küçük UnitId.
- reload_ticks dolunca atış; anında isabet.
- Etkin zırh = zırh_mm / cos(vuruş açısı); ön/yan/arka bölge hedefin yönüne göre.
- pen >= etkin zırh → imha; >= %70 → hasarlı (hız yarıya); altı → etkisiz. Piyade zırhsız, hp ile (damage_hp).
- Aynı tick'teki atışlar önce toplanır, sonra birlikte uygulanır.
Kabul: birbirini aynı tick vuran iki tank imha; ön/yan farkı; iz < 3 ateş yok.

## G5 — Tur motoru ve görünürlük filtresi
- sim::turn: plan_phase(world, orders_by_player) → doğrulanmış emirler; resolve_phase(world) → 200 tick + olay listesi.
- KBG: oyuncu başı 8 puan, MoveOrder 1 puan (data/rules.toml). Aşan emir sebebiyle reddedilir.
- Olaylar: UnitMoved, ShotFired, UnitDamaged, UnitDestroyed, TrackGained, TrackLost.
- filter_for_player: kendi birimleri + iz >= 1 düşman olayları. İz 1 olayları kesin konum değil bölge (merkez + yarıçap).
- Hile testi: test içinde sabit sayı üreteciyle 20 senaryo; filtrelenmiş akışta görülmeyen birim kimliği yok.
Kabul: KBG reddi, filtre, hile, tam tur determinizm.

## G6 — Protokol
- ClientMsg: Hello { protocol_version, player_name }, SubmitOrders { turn, orders }, RequestPreview { unit, waypoints }, Ready.
- ServerMsg: Welcome { player_id, map_id }, TurnStart { turn, deadline_secs, view: PlayerView }, Resolution { turn, events: Vec<VisibleEvent> },
  PreviewResult, OrderRejected { reason }, Error { code, message }.
- PlayerView sadece görülenler; protokol tipleri (sim tipleri değil); dönüşüm server/ içinde.
- JSON + zstd; 1 MB üstü hata; protocol_version uyumsuzsa ret. docs/protocol.md: her mesaj için JSON örneği.
Kabul: her mesaj gidiş-dönüş, sürüm uyumsuzluğu testi.

## G7 — Sunucu
- tokio + tokio-tungstenite; 0.0.0.0:7777; --port, --map, --resume <match_id>.
- 2 oyuncu bağlanınca başlar; planlama süresi rules.toml (45 sn); süre dolunca ya da iki Ready'de çözüm.
- Çözüm spawn_blocking ile. Her tur sonu saves/<match_id>/turn_<n>.json.zst; --resume ile devam.
- Kopan oyuncu aynı isimle dönünce güncel TurnStart; maç bekler. Panik yok, tracing log, çözüm süresi loglanır.
Kabul: Rust test istemcileriyle uçtan uca tur; kapat + --resume; kopma ve yeniden bağlanma.

## G8 — Harita aracı (Hasandağı)
- Açık lisanslı DEM indir (hesap gerekirse DUR). Lisans/atıf ASSETS.md'ye.
- docs/maps.md: zirve merkezli ~40x40 km alanı gdalwarp ile metrik projeksiyona çevirme/kesme. Zirve koordinatı doğrulanır.
- tools/: saf Rust GeoTIFF okuyan dönüştürücü (izinli lisans; yoksa DUR, GDAL bağımlılığı yok).
- Çıktı 1: maps/hasandag/sim_height.bin (50 m, f32 LE) + map.toml. Çıktı 2: terrain_r16.raw (16-bit) + içe aktarma notları (min/max).
Kabul: sentetik küçük GeoTIFF dönüşüm testi; gerçek harita boyut ve min/max raporu.

## G9 — Godot istemci
- client/ Godot 4 projesi; Terrain3D (MIT) addons/; terrain_r16.raw içe aktarma. Editörde elle adımlar → kullanıcıya liste, DUR.
- WebSocketPeer (varsayılan 127.0.0.1:7777); zstd aç, JSON ayrıştır.
- Birimler kutu (MultiMeshInstance3D, takım renkleri); düşman son bilinen konumu soluk işaret.
- Planlama: tıkla seç, sağ tık rota noktası, Hazır, geri sayım, kalan KBG. Oynatma: 20 sn zaman çizelgesi, yumuşak geçiş.
- Kural hesabı yok; rota önizleme RequestPreview.
Kabul: iki istemci tam tur oynar; oynatma takılmaz.

## G10 — Faz 0 kapısı
- Hasandağı, taraf başına 6 piyade + 2 tank, sabit emirlerle 5 tur; 3 kez → olay akışları bayt düzeyinde aynı.
- Hile testi: hiçbir oyuncu mesajında görmediği birim kimliği yok.
- Performans: çözüm < 1 sn hedef; istemci FPS. Aşılırsa flamegraph ile en yavaş 3 fonksiyon.
- docs/faz0-rapor.md. Sonra DUR.

# Faz 1 — kusursuz mekanik ve altyapı (2026-10-07)

Kullanıcı kararları: tur = 2 dk oyun süresi; öncelik oynanabilirlik + yol/bölge/ikmal + radar/ESM.
Tasarım belgesi (design.md, K-kimlikleri) ve görsel efektler (roket, patlama, hava) Faz 2'ye kaldı.
Varsayılan sayılar yer tutucudur ve hepsi data/*.toml içindedir.

- F1-0 Ölçek: rules.toml `turn.game_seconds_per_tick = 0.6` (200 adım = 2 dk); units.toml `reload_s` (saniye).
- F1-1 Grup emirleri ve KBG: `move { units, waypoints }` grup emri, maliyet = ceil(birim / 12) puan (rules.toml);
  birimler hedefte grup içindeki göreli düzenini korur. `stop { units }` emri (önceki turdan süren hareketi keser).
- F1-2 Yol ağı: OpenStreetMap (Overpass, ODbL) → tools `roads` → maps/hasandag/roads.bin (hücre başına yol sınıfı)
  + roads_lines.json (istemci çizimi). Yolda `speed_road_mps`; A* yolu tercih eder.
- F1-3 Bölgeler: senaryoda `[[zone]]` (merkez, yarıçap, puan, depo mu). Sadece piyade ele geçirir: bölgede tek tarafın
  canlı piyadesi `capture_s` boyunca kalırsa sahiplik değişir (adım sırasının sonu). Tur sonu: sahibine puan.
  Bölge durumu herkese açık (harita bayrağı).
- F1-4 İkmal: birimlerde yakıt ve mühimmat; hareket yakıt, atış mühimmat harcar; biten durur / ateş edemez.
  `supply` (ikmal kamyonu) birimi tur sonunda yakınındaki dost birimleri doldurur; kendisi depo bölgesinde dolar.
- F1-5 Sensörler: units.toml'da birim başına sensör listesi. `radar` (aktif, açılıp kapatılır, hareketli hedefi
  iz 2 ile görür, açıkken yayın yapar), `esm` (pasif, yayın yapan düşmanın yönünü verir = iz 1 kerteriz; iki ESM
  kerterizi yeterli açıyla kesişirse üçgenleme = iz 2), `jammer` (açıkken yarıçapındaki hedeflerin radar izini düşürür, kendisi de yayın yapar).
  Yeni birimler: radar aracı, ESM aracı, karıştırıcı, ikmal kamyonu. `set_emitter { units, on }` emri.
- F1-6 İstemci: çoklu seçim (sürükle), Ctrl+1–9 grupları, emir silme / durdurma, birim bilgi paneli (hp, yakıt, mühimmat),
  yol ve bölge çizimi, skor, radar aç/kapa, ESM kerteriz çizgileri.
- F1-7 Kapı: Hasandağı senaryosu yeni birimlerle; determinizm ×3, hile testi (kerteriz dahil), süre; docs/faz1-rapor.md.
