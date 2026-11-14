# SPDX-License-Identifier: AGPL-3.0-or-later
# 2D interface: connection bar, turn/KBG/score line, status, unit info panel, drag box, game over panel,
# event log.
extends CanvasLayer

signal connect_pressed(address: String)
signal ready_pressed

const TYPE_NAMES := {
	"infantry": "Piyade",
	"mbt": "Ana Muharebe Tankı",
	"gsr": "Kara Gözetleme Radarı",
	"esm": "ESM Aracı",
	"jammer": "Karıştırıcı",
	"supply": "İkmal Kamyonu",
	"atgm": "Tanksavar Timi",
	"hq": "Karargâh",
	"spg": "Obüs",
	"rocket": "Füze Rampası",
	"ifv": "Zırhlı Muharebe Aracı",
	"recon": "Keşif Aracı",
	"uav": "İHA",
	"suav": "SİHA",
	"shorad": "Alçak İrtifa Hava Savunma",
	"helo": "Taarruz Helikopteri",
	"mg_team": "Makineli Tüfek Timi",
	"rocket_inf": "Roketatar Timi",
	"radar_mobile": "Mobil Arama Radarı",
	"ambulance": "Sağlık Aracı",
	"radar_tower": "Radar Kulesi",
	"field_hospital": "Sahra Hastanesi",
	"logistics_base": "Lojistik Üs",
	"depot": "Depo",
	"airfield": "Hava Alanı",
	"port": "Liman",
	"hq_bunker": "Karargâh Sığınağı",
	"ad_site": "Hava Savunma Mevzisi",
	"cargo_plane": "Kargo Uçağı",
	"decoy_tank": "Sahte Tank",
	"decoy_radar": "Sahte Radar",
	"harpy": "Kamikaze İHA Rampası",
	"cb_radar": "Karşı-Batarya Radarı",
}
# type -> [short label, "your ..." (log), "enemy's ..." (log)]
const TYPE_WORDS := {
	"infantry": ["Piyade", "Piyadeniz", "piyadesi"],
	"mbt": ["Tank", "Tankınız", "tankı"],
	"gsr": ["Radar", "Radarınız", "radarı"],
	"esm": ["ESM", "ESM aracınız", "ESM aracı"],
	"jammer": ["Karıştırıcı", "Karıştırıcınız", "karıştırıcısı"],
	"supply": ["İkmal", "İkmal aracınız", "ikmal aracı"],
	"atgm": ["Tanksavar", "Tanksavarınız", "tanksavarı"],
	"hq": ["Karargâh", "Karargâhınız", "karargâhı"],
	"spg": ["Obüs", "Obüsünüz", "obüsü"],
	"rocket": ["Füze", "Füze rampanız", "füze rampası"],
	"mg_team": ["Makineli", "Makineli timiniz", "makineli timi"],
	"rocket_inf": ["Roketatar", "Roketatar timiniz", "roketatar timi"],
	"radar_mobile": ["Arama Radarı", "Arama radarınız", "arama radarı"],
	"ambulance": ["Sağlık", "Sağlık aracınız", "sağlık aracı"],
	"radar_tower": ["Radar Kulesi", "Radar kuleniz", "radar kulesi"],
	"field_hospital": ["Hastane", "Hastaneniz", "hastanesi"],
	"logistics_base": ["Lojistik Üs", "Lojistik üssünüz", "lojistik üssü"],
	"depot": ["Depo", "Deponuz", "deposu"],
	"airfield": ["Hava Alanı", "Hava alanınız", "hava alanı"],
	"port": ["Liman", "Limanınız", "limanı"],
	"hq_bunker": ["Sığınak", "Karargâh sığınağınız", "karargâh sığınağı"],
	"ad_site": ["HS Mevzisi", "Hava savunma mevziniz", "hava savunma mevzisi"],
	"cargo_plane": ["Kargo Uçağı", "Kargo uçağınız", "kargo uçağı"],
	"decoy_tank": ["Sahte Tank", "Sahte tankınız", "sahte tankı"],
	"decoy_radar": ["Sahte Radar", "Sahte radarınız", "sahte radarı"],
	"harpy": ["Kamikaze İHA", "Kamikaze İHA rampanız", "kamikaze İHA rampası"],
	"cb_radar": ["KB Radarı", "Karşı-batarya radarınız", "karşı-batarya radarı"],
}
const STOCK_TYPES := ["supply", "cargo_plane", "logistics_base", "depot", "airfield", "port"]
const LOG_LINES := 8
const STANCE_NAMES := {"free": "Serbest", "return": "Karşılık", "hold": "Ateş Yok", "ambush": "Pusu"}
const STATE_NAMES := {"healthy": "Sağlam", "damaged": "Hasarlı", "destroyed": "Yok edildi"}

var name_box := LineEdit.new()
var addr := LineEdit.new()
var connect_button := Button.new()
var ready_button := Button.new()
var line := Label.new()
var status := Label.new()
var fps := Label.new()
var info := Label.new()
var info_panel := PanelContainer.new()
var drag_box := ColorRect.new()
var over_panel := PanelContainer.new()
var over_label := Label.new()
var log_box := RichTextLabel.new()
var log_lines: Array = []  # [text, colour, repeat count]


func _ready() -> void:
	var bar := HBoxContainer.new()
	bar.position = Vector2(10, 10)
	add_child(bar)
	addr.text = "127.0.0.1:7777"
	addr.custom_minimum_size.x = 180
	name_box.text = "oyuncu"
	name_box.custom_minimum_size.x = 120
	connect_button.text = "Bağlan"
	connect_button.pressed.connect(func(): connect_pressed.emit(addr.text))
	ready_button.text = "Hazır"
	ready_button.disabled = true
	ready_button.pressed.connect(func(): ready_pressed.emit())
	for c in [addr, name_box, connect_button, ready_button, line]:
		bar.add_child(c)
	status.position = Vector2(10, 50)
	add_child(status)
	fps.position = Vector2(10, 80)
	add_child(fps)
	# Readable on any background: white text with a dark outline.
	for l in [status, line, fps, info]:
		l.add_theme_font_size_override("font_size", 18)
		l.add_theme_color_override("font_outline_color", Color.BLACK)
		l.add_theme_constant_override("outline_size", 6)

	info_panel.grow_vertical = Control.GROW_DIRECTION_BEGIN
	info_panel.set_anchors_and_offsets_preset(Control.PRESET_BOTTOM_LEFT, Control.PRESET_MODE_MINSIZE, 10)
	info_panel.visible = false
	info_panel.mouse_filter = Control.MOUSE_FILTER_IGNORE
	info_panel.add_child(info)
	add_child(info_panel)

	drag_box.color = Color(1, 1, 0.3, 0.15)
	drag_box.visible = false
	drag_box.mouse_filter = Control.MOUSE_FILTER_IGNORE
	add_child(drag_box)

	var log_panel := PanelContainer.new()
	log_panel.grow_horizontal = Control.GROW_DIRECTION_BEGIN
	log_panel.grow_vertical = Control.GROW_DIRECTION_BEGIN
	log_panel.set_anchors_and_offsets_preset(Control.PRESET_BOTTOM_RIGHT, Control.PRESET_MODE_MINSIZE, 10)
	log_panel.mouse_filter = Control.MOUSE_FILTER_IGNORE
	var bg := StyleBoxFlat.new()
	bg.bg_color = Color(0, 0, 0, 0.45)
	bg.set_content_margin_all(8)
	log_panel.add_theme_stylebox_override("panel", bg)
	log_box.bbcode_enabled = true
	log_box.fit_content = true
	log_box.custom_minimum_size = Vector2(460, 0)
	log_box.mouse_filter = Control.MOUSE_FILTER_IGNORE
	log_box.add_theme_font_size_override("normal_font_size", 16)
	log_panel.add_child(log_box)
	add_child(log_panel)

	over_panel.grow_horizontal = Control.GROW_DIRECTION_BOTH
	over_panel.grow_vertical = Control.GROW_DIRECTION_BOTH
	over_panel.set_anchors_and_offsets_preset(Control.PRESET_CENTER)
	over_panel.visible = false
	over_label.add_theme_font_size_override("font_size", 32)
	over_label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	over_panel.add_child(over_label)
	add_child(over_panel)


func set_connected(on: bool) -> void:
	connect_button.disabled = on
	connect_button.text = "Bağlı" if on else "Bağlan"
	name_box.editable = not on
	addr.editable = not on


## Large centred message (also used for the game-over result); empty text hides it.
func banner(text: String) -> void:
	over_label.text = text
	over_panel.visible = not text.is_empty()


func show_drag(a: Vector2, b: Vector2) -> void:
	drag_box.visible = true
	drag_box.position = Vector2(min(a.x, b.x), min(a.y, b.y))
	drag_box.size = (a - b).abs()


func show_game_over(winner, scores: Array, player_id: int) -> void:
	var title := "Berabere"
	if winner != null:
		title = "Kazandınız!" if int(winner) == player_id else "Kaybettiniz"
		title += " (Oyuncu %d)" % (int(winner) + 1)
	over_label.text = "Oyun bitti\n%s\n\n%s" % [title, score_text(scores, player_id)]
	over_panel.visible = true


func score_text(scores: Array, player_id: int) -> String:
	if scores.size() < 2 or player_id < 0:
		return ""
	return "Bölge: biz %d - rakip %d" % [int(scores[player_id]), int(scores[1 - player_id])]


# sel: Array of unit dicts (own units). orders: the pending orders (orders.gd).
func show_selection(sel: Array, orders) -> void:
	info_panel.visible = not sel.is_empty()
	if sel.size() == 1:
		var u: Dictionary = sel[0]
		var lines := [
			"%s  #%d" % [TYPE_NAMES.get(u.type, u.type), u.id],
			"Can: %.0f   Durum: %s" % [u.hp, STATE_NAMES.get(u.state, u.state)],
			"Yakıt: %.0f   Mühimmat: %d" % [u.fuel, u.ammo],
		]
		var emit := "Yayın: %s" % ("Açık" if u.emitting else "Kapalı")
		var on = orders.pending_emitter(u.id)
		if on != null:
			emit += " -> %s (emir)" % ("Açık" if on else "Kapalı")
		lines.append(emit)
		var stance := "Ateş kuralı: %s" % STANCE_NAMES.get(u.stance, u.stance)
		var st = orders.pending_stance(u.id)
		if st != null:
			stance += " -> %s (emir)" % STANCE_NAMES.get(st, st)
		lines.append(stance)
		lines.append("Komuta: %s" % ("bağlı" if u.connected else "KOPUK (emir alamaz)"))
		if u.get("silent", false):
			lines.append("TELSİZ SESSİZLİĞİ (X ile kaldır)")
		if float(u.get("packed_s", 0.0)) > 0.0:
			lines.append("KURULU DEĞİL: %d sn (ateş ve yayın yok)" % ceili(float(u.packed_s)))
		var t = orders.pending_mission(u.id)
		if t != null:
			lines.append("Atış görevi: %s" % ("nokta" if t.kind == "point" else "birim #%d" % t.unit))
		if u.type in STOCK_TYPES:
			lines.append("İkmal stoğu: yakıt %.0f, mühimmat %d" % [u.supply_fuel, u.supply_ammo])
		var conceal := float(u.get("conceal", 1.0))
		if conceal < 1.0:
			lines.append("Gizlenme: düşman %%%d menzilden görür" % roundi(conceal * 100.0))
		var pressure := float(u.get("suppression", 0.0))
		if pressure > 0.0:
			lines.append("Baskı altında: %%%d%s" % [roundi(pressure * 100.0),
				"  YERE YATTI (ilerleyemez)" if u.get("pinned", false) else ""])
		if u.get("carried_by") != null:
			lines.append("Araçta: #%d (inmek için N)" % int(u.carried_by))
		if int(u.get("unsupplied_turns", 0)) > 0:
			lines.append("İKMALSİZ: %d tur (3. turdan itibaren erir)" % int(u.unsupplied_turns))
		info.text = "\n".join(lines)
	elif sel.size() > 1:
		var count := {}
		for u in sel:
			count[u.type] = count.get(u.type, 0) + 1
		var lines := ["%d birim seçili" % sel.size()]
		var cut := sel.filter(func(u): return not u.connected).size()
		if cut > 0:
			lines.append("%d birim KOPUK (emir alamaz)" % cut)
		for t in count:
			lines.append("%d x %s" % [count[t], TYPE_NAMES.get(t, t)])
		info.text = "\n".join(lines)


static func short_name(type: String) -> String:
	return TYPE_WORDS[type][0] if TYPE_WORDS.has(type) else (type.to_upper() if type != "" else "?")


## Unit in a log line: "Tankınız 7" / "Düşman tankı 12"; as an object (dative): "düşman tankı 12'ye".
static func unit_words(type: String, id: int, own: bool, dative := false) -> String:
	var n := dat(id) if dative else str(id)
	if own:
		return "%s %s" % [TYPE_WORDS[type][1] if TYPE_WORDS.has(type) else "Biriminiz", n]
	return "%s %s %s" % ["düşman" if dative else "Düşman", TYPE_WORDS[type][2] if TYPE_WORDS.has(type) else "birimi", n]


## Number with the Turkish dative suffix: 12'ye, 15'e, 9'a, 40'a.
static func dat(n: int) -> String:
	var m := absi(n)
	var tail := "'e"  # 100, 1000: yüz'e, bin'e
	if m == 0:
		tail = "'a"
	elif m % 10 != 0:
		tail = ["", "'e", "'ye", "'e", "'e", "'e", "'ya", "'ye", "'e", "'a"][m % 10]
	elif m % 100 != 0:
		tail = ["", "'a", "'ye", "'a", "'a", "'ye", "'a", "'e", "'e", "'a"][(m / 10) % 10]
	return "%d%s" % [n, tail]


## Adds a line to the event log (bottom right); a repeat of the last line counts up instead.
func log_event(text: String, color := Color.WHITE) -> void:
	if not log_lines.is_empty() and log_lines[-1][0] == text:
		log_lines[-1][2] += 1
	else:
		log_lines.append([text, color, 1])
		if log_lines.size() > LOG_LINES:
			log_lines.pop_front()
	var out := PackedStringArray()
	for l in log_lines:
		out.append("[color=#%s]%s%s[/color]" % [l[1].to_html(false), l[0], " ×%d" % l[2] if l[2] > 1 else ""])
	log_box.text = "\n".join(out)
