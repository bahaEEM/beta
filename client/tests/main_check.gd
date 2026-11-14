# SPDX-License-Identifier: AGPL-3.0-or-later
# Runs the real main scene headless against a live server for one turn (an idle second client
# is the opponent): group move of two units, radar toggle via the R key handler, howitzer stance (F)
# and point fire mission (T), ready, playback with a shell_impact.
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
	if main.ui.status.text.begins_with("Emir reddedildi"):
		printerr("MAIN CHECK FAILED: ", main.ui.status.text)
		quit(1)
	elif step == 0 and main.turn == 1:
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
		# Howitzer: F cycles the stance, T + target (the click handler's end) gives a point mission.
		var spg: Array = main.units.keys().filter(func(id): return main.units[id].owner == main.player_id \
			and main.units[id].type == "spg")
		main.selection = spg
		for k in [KEY_F, KEY_F, KEY_T]:
			key.keycode = k
			main._key(key)
		var aim: Vector2 = main.units[spg[0]].pos + Vector2(8000.0 if main.player_id == 0 else -8000.0, 0)
		main.add_mission({"kind": "point", "pos": aim})
		if main.orders.pending_stance(spg[0]) != "hold" or main.orders.missions().size() != 1:
			printerr("MAIN CHECK FAILED: stance/mission not queued: ", main.orders.list)
			quit(1)
		# Logistics base: U opens the shop, picking the infantry entry queues a purchase.
		main.selection = main.units.keys().filter(func(id): return main.units[id].owner == main.player_id \
			and main.units[id].type == "logistics_base").slice(0, 1)
		key.keycode = KEY_U
		main._key(key)
		var pick: int = main.shop_items.map(func(s): return s.type_id).find("infantry")
		main.ui.shop_menu.id_pressed.emit(pick)
		main._update_hud()
		if main.orders.buys(main.selection[0]).size() != 1 or not "Sipariş: Piyade" in main.ui.info.text:
			printerr("MAIN CHECK FAILED: purchase not queued: ", main.orders.list, main.ui.info.text)
			quit(1)
		step = 1
	elif step == 1 and main.previews.size() == 2 and main.kbg_spent > 0:
		print("previews ", main.previews.keys(), " kbg ", main.kbg_spent, "/", main.kbg_budget,
			" zones ", main.zones.size(), " info:\n", main.ui.info.text)
		main.ui.ready_pressed.emit()
		step = 2
	elif step == 2 and main.turn == 2 and not main.playing:
		var on: Array = main.units.values().filter(func(u): return u.get("emitting", false))
		var impacts: int = main.events.filter(func(e): return e.type == "shell_impact").size()
		var spg: Array = main.units.values().filter(func(u): return u.owner == main.player_id and u.type == "spg")
		print("MAIN OK, emitting units: ", on.map(func(u): return u.id), " impacts: ", impacts,
			" spg stance: ", spg[0].stance, " status: ", main.ui.status.text)
		print("money ", main.money, " +", main.income, " production ", main.production)
		quit(0 if on.size() == 1 and impacts > 0 and spg[0].stance == "hold" and main.production.size() == 1 else 1)
	elif elapsed > 60.0:
		printerr("MAIN CHECK FAILED at step %d, status: %s" % [step, main.ui.status.text])
		quit(1)
	return false
