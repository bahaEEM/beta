# SPDX-License-Identifier: AGPL-3.0-or-later
# Headless protocol smoke test against a running server:
#   godot --headless --path client --script res://tests/smoke.gd -- 127.0.0.1:7777
# Two clients play one turn: a group move of two units (one preview per unit), a radar toggle, a stance
# change and a point fire mission for the howitzer, built with the client's own order bookkeeping
# (orders.gd); each client must see a shell_impact in the resolution. Exits 0 on success.
extends SceneTree

const Orders := preload("res://orders.gd")

var clients: Array = []
var log_lines := {0: [], 1: []}
var previews := {0: 0, 1: 0}
var radar := {0: [-1, false], 1: [-1, false]}  # [unit id, emitting at turn 1]
var impacts := {0: 0, 1: 0}
var paid := {0: -1, 1: -1}  # money expected at turn 2 after buying infantry
var errors: Array = []
var elapsed := 0.0


func _initialize() -> void:
	var args := OS.get_cmdline_user_args()
	var addr: String = args[0] if args.size() > 0 else "127.0.0.1:7777"
	for i in 2:
		var n := Net.new()
		n.opened.connect(func(): n.send({"type": "hello", "protocol_version": Net.PROTOCOL_VERSION, "player_name": "smoke%d" % i}))
		n.message.connect(func(m): _on_message(i, m))
		n.connect_to(addr)
		clients.append(n)


func _on_message(i: int, m: Dictionary) -> void:
	log_lines[i].append(m.type)
	match m.type:
		"turn_start":
			if int(m.turn) == 1:
				_plan(i, m.view.own_units, m.view)
			elif int(m.turn) == 2:
				if m.view.production.size() != 1 or int(m.view.money) != paid[i]:
					errors.append("client %d: purchase not under way: %s, money %s (expected %d)" % [
						i, m.view.production, m.view.money, paid[i]])
				for u in m.view.own_units:
					if int(u.id) == radar[i][0] and u.emitting == radar[i][1]:
						errors.append("client %d: radar %d emitting not toggled" % [i, u.id])
		"preview_result":
			if m.reason == null and not m.path.is_empty():
				previews[i] += 1
		"order_rejected", "error":
			errors.append("client %d: %s" % [i, m])
		"resolution":
			impacts[i] += m.events.filter(func(e): return e.type == "shell_impact").size()
			print("client %d: resolution with %d events, %d shell impacts" % [i, m.events.size(), impacts[i]])


func _plan(i: int, own: Array, view: Dictionary) -> void:
	var movers: Array = own.filter(func(u): return u.type_id == "infantry").slice(0, 2)
	var gsr: Dictionary = own.filter(func(u): return u.type_id == "gsr")[0]
	radar[i] = [int(gsr.id), gsr.emitting]
	var ids: Array = movers.map(func(u): return int(u.id))
	var orders := Orders.new()
	var target := Vector2(movers[0].pos.x + (300.0 if i == 0 else -300.0), movers[0].pos.y)
	orders.add_waypoint(ids, target)
	orders.add_waypoint(ids, target + Vector2(0, 200))  # same selection: same group order
	orders.toggle_emitter({int(gsr.id): gsr.emitting})
	var spg: Dictionary = own.filter(func(u): return u.type_id == "spg")[0]
	orders.set_stance([int(spg.id)], "return")
	orders.set_stance([int(spg.id)], "hold")  # replaces the earlier stance order
	# 8 km towards the enemy: inside the howitzer's range, lands within turn 1 (aim + flight).
	var aim := Vector2(spg.pos.x + (8000.0 if i == 0 else -8000.0), spg.pos.y)
	orders.fire_mission([int(spg.id)], {"kind": "point", "pos": aim})
	# Buy infantry at the first structure that sells it; it must be under way next turn.
	var item: Dictionary = view.shop.filter(func(s): return s.type_id == "infantry")[0]
	orders.buy("infantry", int(item.sites[0]))
	paid[i] = int(view.money) - int(item.cost) + int(view.income)
	assert(orders.list.size() == 5 and orders.moves()[0].waypoints.size() == 2)
	assert(orders.pending_stance(int(spg.id)) == "hold" and orders.missions().size() == 1)
	var mv: Dictionary = orders.to_wire()[0]
	clients[i].send({"type": "request_preview", "units": mv.units, "waypoints": mv.waypoints})
	clients[i].send({"type": "submit_orders", "turn": 1, "orders": orders.to_wire()})
	clients[i].send({"type": "ready"})


func _process(delta: float) -> bool:
	elapsed += delta
	for n in clients:
		n.poll()
	var done := true
	for i in 2:
		done = done and log_lines[i].count("turn_start") >= 2 and log_lines[i].has("resolution") \
			and log_lines[i].has("orders_accepted")
	if not errors.is_empty():
		printerr("SMOKE FAILED ", errors)
		quit(1)
	elif done:
		if previews[0] != 2 or previews[1] != 2:
			printerr("SMOKE FAILED: expected 2 previews per client, got ", previews)
			quit(1)
		elif impacts[0] == 0 or impacts[1] == 0:
			printerr("SMOKE FAILED: no shell_impact in turn 1, got ", impacts)
			quit(1)
		else:
			print("SMOKE OK ", log_lines)
			quit(0)
	elif elapsed > 40.0:
		printerr("SMOKE FAILED (timeout) ", log_lines)
		quit(1)
	return false
