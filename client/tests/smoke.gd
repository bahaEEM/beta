# SPDX-License-Identifier: AGPL-3.0-or-later
# Headless protocol smoke test against a running server:
#   godot --headless --path client --script res://tests/smoke.gd -- 127.0.0.1:7777
# Two clients play one turn: a group move of two units (one preview per unit) and a radar toggle,
# built with the client's own order bookkeeping (orders.gd). Exits 0 on success.
extends SceneTree

const Orders := preload("res://orders.gd")

var clients: Array = []
var log_lines := {0: [], 1: []}
var previews := {0: 0, 1: 0}
var radar := {0: [-1, false], 1: [-1, false]}  # [unit id, emitting at turn 1]
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
				_plan(i, m.view.own_units)
			elif int(m.turn) == 2:
				for u in m.view.own_units:
					if int(u.id) == radar[i][0] and u.emitting == radar[i][1]:
						errors.append("client %d: radar %d emitting not toggled" % [i, u.id])
		"preview_result":
			if m.reason == null and not m.path.is_empty():
				previews[i] += 1
		"order_rejected", "error":
			errors.append("client %d: %s" % [i, m])
		"resolution":
			print("client %d: resolution with %d events" % [i, m.events.size()])


func _plan(i: int, own: Array) -> void:
	var movers: Array = own.filter(func(u): return u.type_id == "infantry").slice(0, 2)
	var gsr: Dictionary = own.filter(func(u): return u.type_id == "gsr")[0]
	radar[i] = [int(gsr.id), gsr.emitting]
	var ids: Array = movers.map(func(u): return int(u.id))
	var orders := Orders.new()
	var target := Vector2(movers[0].pos.x + (300.0 if i == 0 else -300.0), movers[0].pos.y)
	orders.add_waypoint(ids, target)
	orders.add_waypoint(ids, target + Vector2(0, 200))  # same selection: same group order
	orders.toggle_emitter({int(gsr.id): gsr.emitting})
	assert(orders.list.size() == 2 and orders.moves()[0].waypoints.size() == 2)
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
		else:
			print("SMOKE OK ", log_lines)
			quit(0)
	elif elapsed > 40.0:
		printerr("SMOKE FAILED (timeout) ", log_lines)
		quit(1)
	return false
