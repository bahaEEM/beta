# beta — tasarım belgesi

Deterministik, zarsız, eşzamanlı turlu (WEGO) elektronik harp strateji oyunu. Bu belge kuralların tek kaynağıdır;
kodla çelişirse belge kazanır. Kural değişince önce bu belge, sonra kod ve testler güncellenir.
Bütün sayılar `data/units.toml` ve `data/rules.toml` içindedir; burada yalnızca anlamları ve şu anki değerleri yazar.
Yeni testlerin adı kural kimliğiyle başlar (`k043_...`). Eski testler aşağıdaki tabloda kurallarına bağlanmıştır.

## Tur ve zaman
- **K-001 Belirleyicilik.** Aynı başlangıç + aynı emirler = bayt düzeyinde aynı sonuç. Rastgelelik yok, simülasyon saat okumaz,
  birimler kimlik sırasıyla işlenir, sıralı kaplar kullanılır.
- **K-002 Tur.** Planlama (45 sn gerçek zaman; iki oyuncu "Hazır" derse erken biter) → çözüm 200 adım → tur sonu.
  Bir adım 0,6 sn oyun zamanıdır: bir tur = 2 dk oyun, ekranda 20 sn oynatılır.
- **K-003 Adım sırası.** hareket → sensör/iz → ateş (eşzamanlı) → hasar → ele geçirme. Tur sonu: puan → ikmal → zafer kontrolü.

## Emirler ve KBG
- **K-010 KBG bütçesi.** Oyuncu başına tur başı 8 puan. Her emir, içindeki her başlayan 12 birim için 1 puan
  (1–12 birim = 1, 13–24 = 2). Bütçeyi aşan emir gerekçesiyle reddedilir; sonraki ucuz emirler yine geçerli olabilir.
- **K-011 Emir türleri.** `move {units, waypoints}` (grup; birimler merkeze göre düzenini korur, ulaşılamazsa tam noktaya gider),
  `stop {units}`, `set_emitter {units, on}` (sadece radar/karıştırıcı), `set_stance {units, stance}`.
  Bir birim tur başına hareket, yayın ve ateş kuralı türlerinin her birinden en fazla bir emir alabilir.
  Önceki turdan süren rota yeni emir ya da `stop` gelene kadar devam eder.

## Hareket
- **K-020 Yol bulma.** 50 m ızgarada A*, 8 komşu sabit sırada, tamsayı maliyet (ms). Eğimi `max_slope_deg` üstündeki kenar geçilmez.
- **K-021 Yol ağı.** İki yol hücresi arasındaki kenar `speed_road_mps` ile, diğerleri `speed_offroad_mps` ile; A* hızlı yolu tercih eder.
- **K-022 Hasarlı hız.** Hasarlı birim yarı hızla gider (`damaged_speed_factor`).
- **K-023 Yakıt.** Araç km başına `fuel_l_per_km` yakar; yakıtı biten durur. Piyade yakıt kullanmaz. Çarpışma yok.

## Algılama
- **K-030 Görüş hattı.** Yükseklik haritasında yarım hücre adımlarla ışın; göz/hedef yüksekliği birim tipinden.
- **K-031 Optik.** Menzil içinde ve görüş açıksa iz 3; menzilin 1,5 katına kadar iz 1.
- **K-032 Radar.** Sadece açıkken çalışır ve yayın yapar; görüş hattındaki, en az `min_target_speed_mps` hızla hareket eden hedefi iz 2 ile görür.
- **K-033 ESM.** Pasif. Yayın yapan düşmana görüş varsa yön (kerteriz) verir = iz 1, konum yok. İki kerteriz en az 20° kesişirse
  üçgenleme = iz 2 (konum + sınıf).
- **K-034 Karıştırma.** Açık karıştırıcı, yarıçapı içindeki düşman radarlarının verdiği izi 1 düşürür; kendisi de yayın yapar (ESM duyar).
- **K-035 İz tablosu.** Her adım her oyuncu için en iyi sensör sonucu; iz düşünce son bilinen konum (ya da son kerteriz) saklanır.
  Yok edilen birim gözlemlemez ve gözlenmez.

## Muharebe (zarsız)
- **K-040 Hedef seçimi.** Atıcının tarafının iz 3 tuttuğu, silah menzilindeki ve atıcının kendi görüş hattındaki en yakın düşman;
  eşitlikte küçük kimlik.
- **K-041 Nişan süresi.** Aynı hedef `aim_s` boyunca nişanda tutulmadan ilk atış olmaz; hedef değişince sıfırlanır.
- **K-042 Yeniden dolum ve mühimmat.** Atışlar arası `reload_s`; her atış 1 mühimmat harcar (hedef aynı adımda ölmüş olsa bile); mühimmatı biten ateş edemez.
- **K-043 Beklenen hasar.** Hasar = `damage_hp × isabet(mesafe) × delme`. İsabet, menzil boyunca `hit_close`'dan `hit_far`'a doğrusal düşer.
  Delme: penetrasyon ≥ etkin zırh → 1; ≥ %70 → 0,25; altı → 0. Etkin zırh = zırh / cos(vuruş açısı); ön/yan/arka ±45° dilimleri.
- **K-044 Hasar durumu.** Can ≤ %50 → hasarlı; 0 → imha.
- **K-045 Eşzamanlılık.** Aynı adımdaki bütün atışlar önceki durumdan seçilir, sonra birlikte uygulanır.
- **K-046 Ateş kuralları.** `free` serbest; `return` sadece son 60 sn içinde ateş altında kaldıysa; `hold` ateş yok.

## Görünürlük
- **K-050 Görünürlük filtresi.** Oyuncuya giden hiçbir mesaj, o an göremediği birimin kimliğini veya konumunu içermez.
  İz 1 = 500 m'lik hücrenin merkezi; kerterizle bilinen birim için sadece yön; iz ≥ 2 = kesin konum, iz ≥ 2'de tip.
  Kayıp temas son bilinen bölge (ya da son kerteriz) olarak gösterilir. Bölge sahiplikleri herkese açıktır.

## Bölgeler, ekonomi, ikmal
- **K-060 Ele geçirme.** Bölgede tek bir tarafın canlı piyadesi 60 sn kalırsa bölge o tarafa geçer; çekişmede ilerleme donar, boşalınca sıfırlanır.
- **K-061 Puan ve zafer.** Tur sonunda her bölge sahibine puan yazar. 100 puana ulaşan ya da 100. tur sonunda önde olan kazanır; eşitlik berabere.
- **K-062 İkmal.** Tur sonunda ikmal kamyonu 300 m içindeki dost birimlerin yakıt ve mühimmatını stokundan doldurur;
  kendisi kendi depo bölgesinde tam dolar.

## Haberleşme
- **K-080 Komuta ağı.** Her tarafın karargâhı (`hq`) ağın köküdür. Birim, karargâha bir bağlantı zinciriyle ulaşabiliyorsa bağlıdır;
  zincir başka birimlerden geçebilir (röle). Bağlılık her planlama fazının başında hesaplanır.
- **K-081 Telsiz.** İki birim, aradaki mesafe ikisinin `radio_range_m` değerinin küçüğünden kısaysa telsizle bağlanır.
  Açık bir düşman karıştırıcısının `radio_range_m` yarıçapındaki birimin telsizi çalışmaz.
- **K-082 Kablolu hat.** Yol hücresine 150 m'den yakın birim o yolun ağına bağlanır; aynı yol ağındaki iki birim kabloyla bağlıdır. Kablo karıştırılamaz.
- **K-083 Kopuk birim.** Bağlı olmayan birim yeni emir almaz (emir gerekçesiyle reddedilir; grup emrinde tek kopuk birim bütün emri reddettirir);
  eski rotasına ve ateş kuralına devam eder. Karargâh yok olursa hiçbir birim emir alamaz.
- **K-084 Kapsam.** Karargâhla başlamayan taraf için bu kural uygulanmaz (testler ve eğitim senaryoları).

## Sunucu ve protokol
- **K-070 Yetkili sunucu.** Bütün kurallar sunucuda; istemci sadece gösterir ve emir yollar.
- **K-071 Protokol.** JSON + zstd, en fazla 1 MB, sürüm uyuşmazlığında ret (`docs/protocol.md`).
- **K-072 Kayıt.** Her tur sonunda maç durumu kaydedilir (harita hariç); `--resume` ile devam; kopan oyuncu aynı isimle döner.

## Denge (2026-10-07)
Hedefler: ilk temas 2.–3. turda; tank düellosu yaklaşık yarım tur; tanksavar pususu tankı 1–2 isabette düşürür;
piyade çatışması birkaç tur. Ölçüm: `cargo test --release -p server --test balance -- --ignored --nocapture`.
Son ölçüm (Hasandağı, iki taraf 3 tur ilerliyor): ilk atış 3. turda, 10 turda P0 2/2 tank ve 2/2 tanksavar, P1 2/2 tank ve 1/2 tanksavar kaybetti.
Bütün sayılar yer tutucudur; görsel aşamadan önce oyun testiyle ayarlanacak.

## Kurallar ↔ testler
| Kural | Testler |
|---|---|
| K-001 | g2_200_ticks_bit_identical, g5_full_turn_is_deterministic, g5_game_state_round_trips_through_json, g10_three_network_runs_are_byte_identical |
| K-010 | g5_kbg_budget_exceeded_rejected, f1_1_group_order_costs_one_point_per_12_units |
| K-011 | g5_rejects_foreign_and_duplicate_orders, f1_1_group_keeps_formation, f1_1_stop_cancels_route_and_duplicates_rejected, f1_1_move_and_emitter_orders_are_independent, f1_5_set_emitter_rejects_non_emitters |
| K-020 | g2_flat_map_straight_line_and_timing, g2_flat_map_diagonal_is_octile_shortest, g2_steep_ridge_routes_through_gap, g2_errors |
| K-021 | f1_2_path_prefers_faster_road_detour, f1_2_road_speed_on_road |
| K-022 / K-023 | f1_4_moving_uses_fuel_and_empty_tank_stops |
| K-030 / K-031 | g3_hill_blocks_line_of_sight, g3_optical_range_boundaries |
| K-032 | f1_5_radar_sees_moving_targets_only_when_on |
| K-033 | f1_5_one_bearing_reveals_direction_not_position, f1_5_two_crossing_bearings_triangulate |
| K-034 | f1_5_enemy_jammer_degrades_radar |
| K-035 | g3_lost_track_keeps_last_known_pos, g3_destroyed_units_do_not_observe, f1_5_destroyed_units_neither_emit_nor_listen |
| K-040 | k040_nearest_target_then_lowest_id, k040_no_fire_below_track_quality_3_or_out_of_range |
| K-041 | k041_first_shot_only_after_aim_time_then_reload |
| K-042 | k042_no_ammo_no_fire_and_each_shot_uses_a_round |
| K-043 | k043_expected_damage_falls_with_range_and_armour, k043_side_hit_hurts_more_than_front_with_partial_penetration, k043_effective_armour_by_facing |
| K-044 | k044_damage_states_by_hp |
| K-045 | k045_simultaneous_fire_both_take_damage_same_tick |
| K-046 | k046_stances_hold_and_return_fire |
| K-050 | g5_filter_hides_unseen_and_blurs_contacts, g5_cheat_no_unseen_unit_in_filtered_stream_20_scenarios, g10_cheat_oracle_and_resolution_time, g7_end_to_end_turn |
| K-060 | f1_3_infantry_captures_after_capture_time, f1_3_only_infantry_captures_and_contest_freezes |
| K-061 | f1_3_owner_scores_each_turn_and_wins_at_victory_points |
| K-062 | f1_4_supply_refills_nearby_units_and_restocks_at_depot |
| K-080 | k080_radio_links_relay_through_units, k082_no_hq_no_command |
| K-081 / K-082 | k081_enemy_jammer_cuts_radio_but_not_wire |
| K-083 | k083_cut_off_units_reject_new_orders_but_keep_moving |
| K-084 | k084_sides_without_hq_are_not_restricted |
| K-071 | g6_client_messages_round_trip, g6_server_messages_round_trip, g6_version_mismatch_rejected, g6_oversized_message_rejected, g6_protocol_md_up_to_date |
| K-072 | g7_resume_after_shutdown, g7_reconnect_same_name, g7_planning_deadline_starts_resolution |

Veri ve araç testleri (g1_, g8_, f1_2_ projeksiyon/rasterleme) kurala değil, veri bütünlüğüne bağlıdır.
