# SPDX-License-Identifier: AGPL-3.0-or-later
# Pending orders of this turn (client-side bookkeeping only; the server validates and costs them).
# Each order: {"type": "move", "units": [ids], "waypoints": [Vector2]} | {"type": "stop", "units": [ids]}
#             | {"type": "set_emitter", "units": [ids], "on": bool}
#             | {"type": "set_stance", "units": [ids], "stance": "free"|"return"|"hold"}
#             | {"type": "fire_mission", "units": [ids], "target": {"kind": "point", "pos": Vector2} | {"kind": "unit", "unit": id}}
#             | {"type": "embark", "units": [ids], "carrier": id} | {"type": "disembark", "units": [ids]}
#             | {"type": "set_silence", "units": [ids], "on": bool}
#             | {"type": "buy", "type_id": String, "site": id}   (no units)
extends RefCounted

var list: Array = []


func clear() -> void:
	list.clear()


# Adds a waypoint to the group move order of exactly these units, or starts a new one
# (the units leave any other move/stop order they were in).
func add_waypoint(sel: Array, p: Vector2) -> void:
	for o in list:
		if o.type == "move" and _same(o.units, sel):
			o.waypoints.append(p)
			return
	_remove_units(sel, ["move", "stop"])
	list.append({"type": "move", "units": sel.duplicate(), "waypoints": [p]})


func stop(sel: Array) -> void:
	_remove_units(sel, ["move", "stop"])
	list.append({"type": "stop", "units": sel.duplicate()})


# emitting: id -> current emitting state of each eligible unit. A unit that already has a
# pending set_emitter order loses it (toggled back); the others get on = !current.
func toggle_emitter(emitting: Dictionary) -> void:
	var by_state := {true: [], false: []}
	for id in emitting:
		if pending_emitter(id) == null:
			by_state[not emitting[id]].append(id)
	_remove_units(emitting.keys(), ["set_emitter"])
	for on in by_state:
		if not by_state[on].is_empty():
			list.append({"type": "set_emitter", "units": by_state[on], "on": on})


# One stance order for the selection; replaces earlier stance orders of these units.
func set_stance(sel: Array, stance: String) -> void:
	_remove_units(sel, ["set_stance"])
	list.append({"type": "set_stance", "units": sel.duplicate(), "stance": stance})


# One fire mission for the selection; replaces earlier missions of these units.
func fire_mission(sel: Array, target: Dictionary) -> void:
	_remove_units(sel, ["fire_mission"])
	list.append({"type": "fire_mission", "units": sel.duplicate(), "target": target})


# Infantry boards a carrier (the server checks distance and room); replaces earlier boarding orders of these units.
func embark(sel: Array, carrier: int) -> void:
	_remove_units(sel, ["embark", "disembark"])
	list.append({"type": "embark", "units": sel.duplicate(), "carrier": carrier})


func disembark(sel: Array) -> void:
	_remove_units(sel, ["embark", "disembark"])
	list.append({"type": "disembark", "units": sel.duplicate()})


# Radio silence on or off for the selection; replaces earlier silence orders of these units.
func set_silence(sel: Array, on: bool) -> void:
	_remove_units(sel, ["set_silence"])
	list.append({"type": "set_silence", "units": sel.duplicate(), "on": on})


# Buys a unit at a structure (the server checks money and the structure).
func buy(type_id: String, site: int) -> void:
	list.append({"type": "buy", "type_id": type_id, "site": site})


# Pending purchases, of one structure or (site < 0) of all.
func buys(site := -1) -> Array:
	return list.filter(func(o): return o.type == "buy" and (site < 0 or o.site == site))


func cancel_buys(site: int) -> void:
	list = list.filter(func(o): return o.type != "buy" or o.site != site)


func remove(sel: Array) -> void:
	_remove_units(sel, ["move", "stop", "set_emitter", "set_stance", "fire_mission", "embark", "disembark", "set_silence"])


# Pending stance for a unit, or null.
func pending_stance(id: int):
	for o in list:
		if o.type == "set_stance" and id in o.units:
			return o.stance
	return null


# Pending fire mission target for a unit, or null.
func pending_mission(id: int):
	for o in list:
		if o.type == "fire_mission" and id in o.units:
			return o.target
	return null


# Pending set_emitter value for a unit, or null.
func pending_emitter(id: int):
	for o in list:
		if o.type == "set_emitter" and id in o.units:
			return o.on
	return null


func has_stop(id: int) -> bool:
	return list.any(func(o): return o.type == "stop" and id in o.units)


func moves() -> Array:
	return list.filter(func(o): return o.type == "move")


func missions() -> Array:
	return list.filter(func(o): return o.type == "fire_mission")


func to_wire() -> Array:
	var out := []
	for o in list:
		var w: Dictionary = o.duplicate()
		if o.type == "move":
			w.waypoints = o.waypoints.map(func(p): return {"x": p.x, "y": p.y})
		elif o.type == "fire_mission" and o.target.kind == "point":
			w.target = {"kind": "point", "pos": {"x": o.target.pos.x, "y": o.target.pos.y}}
		out.append(w)
	return out


func _remove_units(ids: Array, types: Array) -> void:
	for o in list:
		if o.type in types:
			o.units = o.units.filter(func(u): return not (u in ids))
	list = list.filter(func(o): return o.type == "buy" or not o.units.is_empty())


func _same(a: Array, b: Array) -> bool:
	var x := a.duplicate()
	var y := b.duplicate()
	x.sort()
	y.sort()
	return x == y
