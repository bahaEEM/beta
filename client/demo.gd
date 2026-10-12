# SPDX-License-Identifier: AGPL-3.0-or-later
# --demo: a fake planning state without a server, for checking visuals (godot --path client -- --demo).
# Every unit type of both sides near the camera, damaged and destroyed units, pending orders with
# previews, and a loop of fake combat events (effects + event log). Nothing here is sent anywhere.
extends RefCounted

const TYPES := ["infantry", "atgm", "mbt", "spg", "rocket", "gsr", "esm", "jammer", "supply", "hq",
	"ifv", "recon", "shorad", "uav", "suav", "helo"]
const CENTRE := Vector2(21200, 12650)
const SPACING := 110.0


static func start(main, crowd: bool) -> void:
	main.player_id = 0
	main._on_message({"type": "welcome", "player_id": 0, "map_id": "hasandag"})
	var own: Array = []
	var contacts: Array = []
	for i in TYPES.size():
		var off := Vector2((i % 8 - 3.5) * SPACING, (i / 8) * SPACING)
		var hp := 100.0
		var state := "healthy"
		match TYPES[i]:
			"mbt":
				hp = 38.0
				state = "damaged"
			"recon":
				hp = 0.0
				state = "destroyed"
		own.append({"id": i + 1, "type_id": TYPES[i], "pos": _xy(CENTRE + off - Vector2(0, 200)), "heading_rad": PI / 2,
			"hp": hp, "state": state, "emitting": TYPES[i] in ["gsr", "jammer"], "fuel": 800.0, "ammo": 30,
			"supply_fuel": 0.0, "supply_ammo": 0, "stance": "free", "connected": TYPES[i] != "esm"})
		contacts.append({"unit": i + 31, "quality": 3, "type_id": TYPES[i], "last_seen_tick": 0,
			"position": {"kind": "exact", "pos": _xy(CENTRE + Vector2(off.x, -off.y) + Vector2(0, 200))}})
	if crowd:  # --bench --demo: about 60 units
		for i in 30:
			own.append({"id": 100 + i, "type_id": ["infantry", "mbt", "ifv"][i % 3],
				"pos": _xy(CENTRE + Vector2((i % 10 - 4.5) * SPACING, -700 - (i / 10) * SPACING)), "heading_rad": PI / 2,
				"hp": 100.0, "state": "healthy", "emitting": false, "fuel": 800.0, "ammo": 30,
				"supply_fuel": 0.0, "supply_ammo": 0, "stance": "free", "connected": true})
	contacts.append({"unit": 90, "quality": 1, "type_id": null, "last_seen_tick": 0,
		"position": {"kind": "area", "center": _xy(CENTRE + Vector2(900, 900)), "radius_m": 250.0}})
	var zone := {"id": 1, "name_tr": "Batı Köyü", "center": _xy(CENTRE + Vector2(-750, 0)), "radius_m": 300.0,
		"points": 2, "depot": false, "owner": null}
	main._on_message({"type": "turn_start", "turn": 3, "deadline_secs": 60, "view": {"own_units": own,
		"contacts": contacts, "kbg_budget": 8, "zones": [zone], "scores": [14, 9]}})
	for id in main.units:  # enemy heading and damage come with events in a real game
		if main.units[id].owner == 1:
			main.units[id].heading = -PI / 2
		if id == 31 + TYPES.find("supply"):
			main.units[id].state = "damaged"
	# Pending orders: a group move with server-like previews, stop, stance, emitter, fire missions.
	var movers := [3, 11]
	main.orders.add_waypoint(movers, CENTRE + Vector2(-350, -150))
	for id in movers:
		var p: Vector2 = main.units[id].pos
		main.previews[id] = [p + Vector2(-60, 120), p + Vector2(-300, 160), p + Vector2(-420, 240)]
	main.orders.stop([1])
	main.orders.set_stance([2], "hold")
	main.orders.toggle_emitter({6: true})
	main.orders.fire_mission([4], {"kind": "point", "pos": CENTRE + Vector2(250, 900)})
	main.orders.fire_mission([5], {"kind": "unit", "unit": 31 + TYPES.find("hq")})
	main.selection = [3, 11]
	main.cam_target = Vector3(CENTRE.x, 0, CENTRE.y)
	main.cam_dist = 1500.0
	main.cam_yaw = 0.35
	var tree: SceneTree = main.get_tree()
	_event(main, {"type": "unit_destroyed", "unit": 31 + TYPES.find("atgm")})
	# A burst just before the --shot screenshot (taken at 4 s), then a loop for live viewing.
	tree.create_timer(3.85).timeout.connect(func(): _burst(main, 0))
	var n := [1]
	var timer := Timer.new()
	timer.wait_time = 1.4
	timer.autostart = true
	timer.timeout.connect(func():
		_burst(main, n[0])
		n[0] += 1)
	main.add_child(timer)


static func _burst(main, n: int) -> void:
	var pairs := [[3, 33], [33, 3], [1, 31], [31, 1], [2, 33], [11, 41]]
	for k in 3:
		var p: Array = pairs[(n + k) % pairs.size()]
		_event(main, {"type": "shot_fired", "shooter": p[0], "target": p[1]})
	_event(main, {"type": "shell_fired", "shooter": 4})
	var hit := CENTRE + Vector2(-100 + 150 * (n % 4), 330 + 40 * (n % 3))
	_event(main, {"type": "shell_impact", "pos": _xy(hit), "radius_m": 60.0})
	if n == 2:
		_event(main, {"type": "zone_captured", "zone": 1, "player": 0})


static func _event(main, e: Dictionary) -> void:
	e["tick"] = 0
	main._apply_event(e)


static func _xy(p: Vector2) -> Dictionary:
	return {"x": p.x, "y": p.y}
