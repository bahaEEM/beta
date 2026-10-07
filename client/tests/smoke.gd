# SPDX-License-Identifier: AGPL-3.0-or-later
# Headless protocol smoke test against a running server:
#   godot --headless --path client --script res://tests/smoke.gd -- 127.0.0.1:7777
# Two clients play one turn. Exits 0 on success.
extends SceneTree

var clients: Array = []
var log_lines := {0: [], 1: []}
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
	if m.type == "turn_start" and int(m.turn) == 1:
		var u = m.view.own_units[0]
		clients[i].send({"type": "submit_orders", "turn": 1, "orders": [{"type": "move", "units": [int(u.id)],
			"waypoints": [{"x": u.pos.x + 300.0, "y": u.pos.y}]}]})
		clients[i].send({"type": "ready"})
	if m.type == "resolution":
		print("client %d: resolution with %d events" % [i, m.events.size()])


func _process(delta: float) -> bool:
	elapsed += delta
	for n in clients:
		n.poll()
	var done := true
	for i in 2:
		done = done and log_lines[i].count("turn_start") >= 2 and log_lines[i].has("resolution")
	if done:
		print("SMOKE OK ", log_lines)
		quit(0)
	elif elapsed > 20.0:
		printerr("SMOKE FAILED ", log_lines)
		quit(1)
	return false
