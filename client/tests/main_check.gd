# SPDX-License-Identifier: AGPL-3.0-or-later
# Runs the real main scene headless against a live server for one turn (an idle second client
# is the opponent): group move of two units, radar toggle via the R key handler, ready, playback.
#   godot --headless --path client --script res://tests/main_check.gd -- 127.0.0.1:7777
# Exits 0 on success; check the output for SCRIPT ERROR as well.
extends SceneTree

var main
var rival := Net.new()
var step := 0
var elapsed := 0.0


func _initialize() -> void:
	var addr: String = OS.get_cmdline_user_args()[0] if OS.get_cmdline_user_args().size() > 0 else "127.0.0.1:7777"
	main = load("res://main.tscn").instantiate()
	root.add_child(main)
	main.net.connect_to(addr)
	rival.opened.connect(func(): rival.send({"type": "hello", "protocol_version": Net.PROTOCOL_VERSION, "player_name": "rival"}))
	rival.message.connect(func(m):
		if m.type == "turn_start":
			rival.send({"type": "ready"}))
	rival.connect_to(addr)
	Engine.time_scale = 8.0  # fast playback


func _process(delta: float) -> bool:
	elapsed += delta
	rival.poll()
	if step == 0 and main.turn == 1:
		var inf: Array = main.units.keys().filter(func(id): return main.units[id].owner == main.player_id \
			and main.units[id].type == "infantry")
		main.selection = inf.slice(0, 2)
		main.orders.add_waypoint(main.selection, main.units[inf[0]].pos + Vector2(300, 0))
		main._orders_changed()
		var gsr: Array = main.units.keys().filter(func(id): return main.units[id].owner == main.player_id \
			and main.units[id].type == "gsr")
		main.selection = gsr
		var key := InputEventKey.new()
		key.keycode = KEY_R
		key.pressed = true
		main._key(key)
		step = 1
	elif step == 1 and main.previews.size() == 2 and main.kbg_spent > 0:
		print("previews ", main.previews.keys(), " kbg ", main.kbg_spent, "/", main.kbg_budget,
			" zones ", main.zones.size(), " info:\n", main.ui.info.text)
		main.ui.ready_pressed.emit()
		step = 2
	elif step == 2 and main.turn == 2 and not main.playing:
		var on: Array = main.units.values().filter(func(u): return u.get("emitting", false))
		print("MAIN OK, emitting units: ", on.map(func(u): return u.id), " status: ", main.ui.status.text)
		quit(0 if on.size() == 1 else 1)
	elif elapsed > 60.0:
		printerr("MAIN CHECK FAILED at step %d, status: %s" % [step, main.ui.status.text])
		quit(1)
	return false
