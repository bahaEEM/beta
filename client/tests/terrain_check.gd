# SPDX-License-Identifier: AGPL-3.0-or-later
# Checks that Terrain3D heights match the simulation grid at a few points.
#   godot --headless --path client --script res://tests/terrain_check.gd
extends SceneTree

const POINTS := [[19850.0, 20100.0, 3261.0], [15000.0, 25600.0, 1665.0], [17600.0, 26600.0, 1467.0]]


func _initialize() -> void:
	var main = load("res://main.tscn").instantiate()
	root.add_child(main)
	await process_frame
	var ok := true
	for p in POINTS:
		var h: float = main.ground(Vector2(p[0], p[1]))
		print("(%d, %d): terrain %.1f m, sim %.1f m" % [p[0], p[1], h, p[2]])
		ok = ok and absf(h - p[2]) < 40.0
	print("TERRAIN OK" if ok else "TERRAIN MISMATCH")
	quit(0 if ok else 1)
