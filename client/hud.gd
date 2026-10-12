# SPDX-License-Identifier: AGPL-3.0-or-later
# 2D interface: connection bar, turn/KBG/score line, status, unit info panel, drag box, game over panel.
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
}
const STANCE_NAMES := {"free": "Serbest", "return": "Karşılık", "hold": "Ateş Yok"}
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
	return "Skor: biz %d - rakip %d" % [int(scores[player_id]), int(scores[1 - player_id])]


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
		var t = orders.pending_mission(u.id)
		if t != null:
			lines.append("Atış görevi: %s" % ("nokta" if t.kind == "point" else "birim #%d" % t.unit))
		if u.type == "supply":
			lines.append("İkmal stoğu: yakıt %.0f, mühimmat %d" % [u.supply_fuel, u.supply_ammo])
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
