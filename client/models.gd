# SPDX-License-Identifier: AGPL-3.0-or-later
# Procedural low-poly unit models (display only). Parts are written in real metres with the
# front towards -Z, merged into one vertex-coloured ArrayMesh per (type, team) and scaled by SCALE.
extends RefCounted

const SCALE := 4.0  # exaggeration so vehicles read on a 40 km map
const AIR_TYPES := ["uav", "suav", "helo"]
const AIR_LIFT := 150.0
const DARK := Color(0.12, 0.13, 0.12)
const METAL := Color(0.42, 0.44, 0.43)
const KHAKI := Color(0.62, 0.56, 0.4)
const SKIN := Color(0.25, 0.3, 0.2)
# Types drawn with another type's model until they get their own.
const LOOKS_LIKE := {"radar_mobile": "gsr", "ambulance": "supply", "mg_team": "atgm", "rocket_inf": "atgm", "ad_site": "shorad"}
const STRUCTURES := ["radar_tower", "field_hospital", "logistics_base", "depot", "airfield", "port", "hq_bunker"]
const DISH_PIVOT := Vector3(0, 7.2, 1.6) * SCALE  # gsr radar head / jammer glow centre (scaled)

static var _cache := {}


## Lift above ground for the type (aircraft fly).
static func lift(type: String) -> float:
	return AIR_LIFT if type in AIR_TYPES else 0.0


## Merged body mesh of a type in the team colour t (cached).
static func body(type: String, t: Color) -> ArrayMesh:
	var key := "%s|%s" % [type, t.to_html()]
	if not _cache.has(key):
		var p: Array = []
		_build(p, type, t)
		_cache[key] = _merge(p)
	return _cache[key]


## Separately animated part (gsr: rotating radar head, origin at DISH_PIVOT), or null.
static func extra(type: String, t: Color) -> ArrayMesh:
	if type != "gsr":
		return null
	var key := "gsr-head|" + t.to_html()
	if not _cache.has(key):
		var p: Array = []
		_part(p, _box(3.4, 1.3, 0.25), Vector3(0, 0.3, 0), METAL.lightened(0.3))
		_part(p, _box(0.6, 0.6, 0.6), Vector3(0, 0, 0.35), t.darkened(0.3))
		_cache[key] = _merge(p)
	return _cache[key]


static func _build(p: Array, type: String, t: Color) -> void:
	var d := t.darkened(0.35)
	if type in STRUCTURES:  # placeholder building: a block with a team-coloured roof (and a mast for the radar tower)
		_part(p, _box(14.0, 5.0, 10.0), Vector3(0, 2.5, 0), KHAKI.darkened(0.15))
		_part(p, _box(14.4, 0.6, 10.4), Vector3(0, 5.3, 0), t)
		if type == "radar_tower":
			_rod(p, Vector3(0, 5.3, 0), Vector3(0, 20.0, 0), 0.5, METAL)
			_part(p, _box(5.0, 2.0, 0.4), Vector3(0, 20.0, 0), METAL.lightened(0.3))
		return
	match LOOKS_LIKE.get(type, type):
		"infantry":
			for o in [Vector2(0, -2.2), Vector2(-2.2, 0), Vector2(2.2, 0), Vector2(0, 2.2)]:
				_soldier(p, Vector3(o.x, 0, o.y), t, 1.5)
		"atgm":
			_soldier(p, Vector3(1.6, 0, 1.2), t, 1.5)
			# Launcher on a tripod, gunner kneeling behind it.
			for a in [0.0, 2.1, 4.2]:
				_rod(p, Vector3(-1, 1.6, -0.6), Vector3(-1 + 1.1 * sin(a), 0, -0.6 + 1.1 * cos(a)), 0.07, DARK)
			_rod(p, Vector3(-1, 1.8, 1.1), Vector3(-1, 2.0, -2.3), 0.28, KHAKI.darkened(0.2))
			_part(p, _box(0.7, 0.5, 0.5), Vector3(-1, 1.7, 0.3), DARK)
			_part(p, _box(0.8, 0.9, 0.55), Vector3(-1, 0.75, 1.6), t)
			_part(p, _sphere(0.25), Vector3(-1, 1.45, 1.6), SKIN)
		"mbt":
			_tracks(p, 3.6, 7.2, t)
			_part(p, _box(2.6, 0.9, 3.2), Vector3(0, 2.15, 0.4), d)
			_part(p, _prism(2.6, 0.9, 0.9), Vector3(0, 2.15, -1.6), d, Vector3(-90, 0, 0))
			_rod(p, Vector3(0, 2.2, -1.2), Vector3(0, 2.35, -7.2), 0.14, DARK)
			_rod(p, Vector3(0, 2.33, -6.6), Vector3(0, 2.36, -7.3), 0.22, DARK)
			_part(p, _cyl(0.4, 0.4, 0.35), Vector3(0.6, 2.75, 0.9), d.darkened(0.2))
			_part(p, _box(0.5, 0.35, 0.5), Vector3(-0.7, 2.75, 1.0), DARK)
		"spg":
			_tracks(p, 3.4, 7.2, t)
			_part(p, _box(3.0, 1.9, 3.8), Vector3(0, 2.6, 1.3), d)
			_rod(p, Vector3(0, 3.0, -0.6), Vector3(0, 7.0, -7.3), 0.17, DARK)
			_rod(p, Vector3(0, 6.6, -6.7), Vector3(0, 7.2, -7.7), 0.26, DARK)
			_part(p, _box(1.0, 0.6, 1.0), Vector3(-0.9, 3.7, 2.2), DARK)
		"rocket":
			_truck(p, 9.0, t)
			var pod := Transform3D(Basis.from_euler(Vector3(deg_to_rad(28), 0, 0)), Vector3(0, 3.3, 1.6))
			_part_xf(p, _box(2.4, 1.5, 4.2), pod, d)
			for i in 6:
				var tube := pod * Vector3(-0.8 + 0.8 * (i % 3), -0.35 + 0.7 * (i / 3), -2.12)
				_part_xf(p, _cyl(0.28, 0.28, 0.08), Transform3D(pod.basis * Basis.from_euler(Vector3(PI / 2, 0, 0)), tube), DARK)
		"gsr":
			_truck(p, 7.5, t)
			_part(p, _box(2.2, 1.8, 3.2), Vector3(0, 2.4, 1.6), d)
			_rod(p, Vector3(0, 3.3, 1.6), Vector3(0, 7.2, 1.6), 0.15, METAL)
		"esm":
			_truck(p, 7.5, t)
			_part(p, _box(2.2, 1.8, 3.2), Vector3(0, 2.4, 1.6), d)
			_rod(p, Vector3(0, 3.3, 1.6), Vector3(0, 8.5, 1.6), 0.1, METAL)
			for y in [5.5, 6.5, 7.5]:
				_rod(p, Vector3(-1.6 + (y - 5.5) * 0.4, y, 1.6), Vector3(1.6 - (y - 5.5) * 0.4, y, 1.6), 0.07, Color.WHITE)
			for x in [-0.9, 0.9]:
				for z in [0.4, 2.8]:
					_rod(p, Vector3(x, 3.3, z), Vector3(x, 5.2, z), 0.06, METAL)
		"jammer":
			_truck(p, 7.5, t)
			_part(p, _box(2.3, 2.0, 3.4), Vector3(0, 2.5, 1.6), d)
			_rod(p, Vector3(0, 3.5, 1.6), Vector3(0, 5.6, 1.6), 0.2, METAL)
			var dish := Transform3D(Basis.from_euler(Vector3(deg_to_rad(-60), 0, 0)), Vector3(0, 6.2, 1.4))
			_part_xf(p, _sphere(1.5, true), dish, Color(0.85, 0.85, 0.8))
			for x in [-1.0, 1.0]:
				_rod(p, Vector3(x, 3.5, 3.0), Vector3(x, 7.5, 3.0), 0.07, METAL)
		"supply":
			_truck(p, 9.0, t)
			_part(p, _box(2.5, 2.3, 5.6), Vector3(0, 2.6, 1.4), KHAKI)
			for z in [-0.6, 1.4, 3.4]:
				_part(p, _box(2.6, 0.15, 0.2), Vector3(0, 3.8, z), KHAKI.darkened(0.3))
		"hq":
			_truck(p, 9.0, t)
			_part(p, _box(2.6, 2.6, 6.0), Vector3(0, 2.7, 1.2), d)
			_part(p, _prism(5.0, 3.0, 5.0), Vector3(5.0, 1.5, 1.0), KHAKI)
			_rod(p, Vector3(-0.9, 4.0, 3.6), Vector3(-0.9, 16.0, 3.6), 0.09, METAL)
			_part(p, _box(0.05, 1.4, 2.4), Vector3(-0.9, 15.2, 2.4), t.lightened(0.25))
			_rod(p, Vector3(0.9, 4.0, 3.6), Vector3(0.9, 8.0, 3.6), 0.07, METAL)
		"ifv":
			_tracks(p, 3.2, 6.6, t)
			_part(p, _box(1.6, 0.7, 1.8), Vector3(0.3, 2.1, -0.4), d)
			_rod(p, Vector3(0.3, 2.2, -1.2), Vector3(0.3, 2.25, -4.0), 0.08, DARK)
		"recon":
			_wheels(p, 5.6, 4)
			_part(p, _box(2.4, 1.3, 5.6), Vector3(0, 1.5, 0), t)
			_part(p, _box(1.2, 0.5, 1.2), Vector3(0, 2.4, 0), d)
			_rod(p, Vector3(0, 2.6, 0.5), Vector3(0, 4.5, 0.5), 0.12, METAL)
		"shorad":
			_tracks(p, 3.2, 6.6, t)
			_part(p, _box(2.4, 1.0, 2.4), Vector3(0, 2.2, 0.8), d)
			_part(p, _box(2.4, 0.9, 1.6), Vector3(0, 3.4, 0.6), DARK, Vector3(25, 0, 0))
			_part(p, _box(1.8, 1.2, 0.15), Vector3(0, 3.6, 2.2), METAL, Vector3(-15, 0, 0))
		"uav", "suav":
			_rod(p, Vector3(0, 0, -3.0), Vector3(0, 0, 3.0), 0.35, t.lightened(0.2))
			_part(p, _box(12.0, 0.12, 1.1), Vector3(0, 0.1, -0.3), t.lightened(0.2))
			_part(p, _box(3.4, 0.1, 0.7), Vector3(0, 0.1, 2.6), d)
			_part(p, _box(0.1, 1.0, 0.7), Vector3(0, 0.6, 2.6), d)
			if type == "suav":
				for x in [-2.5, 2.5]:
					_rod(p, Vector3(x, -0.35, -1.2), Vector3(x, -0.35, 0.8), 0.12, METAL)
		"helo":
			_part(p, _sphere(1.3), Vector3(0, 0, -1.0), t)
			_rod(p, Vector3(0, 0.2, 0), Vector3(0, 0.5, 6.0), 0.3, t)
			for a in [30.0, 120.0]:  # rotor blades
				_part(p, _box(11.0, 0.08, 0.35), Vector3(0, 1.6, -0.8), DARK, Vector3(0, a, 0))
			_part(p, _box(0.1, 1.4, 0.8), Vector3(0, 1.0, 6.0), d)
		_:  # unknown future type: generic tracked box
			_tracks(p, 3.2, 6.6, t)
			_part(p, _box(2.0, 1.0, 2.0), Vector3(0, 2.1, 0), d)


# ---------------------------------------------------------------- composite parts

static func _soldier(p: Array, at: Vector3, t: Color, s: float) -> void:
	_part(p, _box(0.45, 0.8, 0.3) , at + Vector3(0, 0.4, 0) * s, KHAKI.darkened(0.4), Vector3.ZERO, s)
	_part(p, _box(0.6, 0.7, 0.38), at + Vector3(0, 1.15, 0) * s, t, Vector3.ZERO, s)
	_part(p, _sphere(0.2), at + Vector3(0, 1.68, 0) * s, SKIN, Vector3.ZERO, s)
	_rod(p, at + Vector3(0.3, 1.2, 0.2) * s, at + Vector3(0.3, 1.35, -0.8) * s, 0.06 * s, DARK)


# Hull on two tracks; width w, length l.
static func _tracks(p: Array, w: float, l: float, t: Color) -> void:
	for x in [-1, 1]:
		_part(p, _box(0.8, 1.1, l), Vector3(x * (w * 0.5 - 0.4), 0.55, 0), DARK)
	_part(p, _box(w - 1.4, 0.9, l - 0.2), Vector3(0, 1.0, 0), t.darkened(0.15))
	_part(p, _box(w, 0.5, l - 0.6), Vector3(0, 1.4, 0), t)


static func _wheels(p: Array, l: float, n: int) -> void:
	for i in n / 2:
		var z := -l * 0.38 + l * 0.76 * i / maxf(n / 2 - 1, 1)
		for x in [-1.15, 1.15]:
			_part(p, _cyl(0.55, 0.55, 0.45, 8), Vector3(x, 0.55, z), DARK, Vector3(0, 0, 90))


# Truck: wheels, chassis and cab at the front; the cargo bed is z in [-l/2 + 2, l/2].
static func _truck(p: Array, l: float, t: Color) -> void:
	_wheels(p, l, 6 if l < 8.5 else 8)
	_part(p, _box(2.3, 0.4, l), Vector3(0, 1.1, 0), DARK)
	_part(p, _box(2.5, 2.0, 1.9), Vector3(0, 2.2, -l * 0.5 + 0.95), t)
	_part(p, _box(2.3, 0.6, 0.1), Vector3(0, 2.6, -l * 0.5 - 0.02), Color(0.15, 0.2, 0.25))


# ---------------------------------------------------------------- primitives

static func _box(x: float, y: float, z: float) -> BoxMesh:
	var m := BoxMesh.new()
	m.size = Vector3(x, y, z)
	return m


static func _cyl(top: float, bottom: float, h: float, seg := 8) -> CylinderMesh:
	var m := CylinderMesh.new()
	m.top_radius = top
	m.bottom_radius = bottom
	m.height = h
	m.radial_segments = seg
	m.rings = 1
	return m


static func _sphere(r: float, hemi := false) -> SphereMesh:
	var m := SphereMesh.new()
	m.radius = r
	m.height = r if hemi else r * 2.0
	m.is_hemisphere = hemi
	m.radial_segments = 8
	m.rings = 4
	return m


static func _prism(x: float, y: float, z: float) -> PrismMesh:
	var m := PrismMesh.new()
	m.size = Vector3(x, y, z)
	return m


static func _part(p: Array, m: PrimitiveMesh, pos: Vector3, c: Color, rot_deg := Vector3.ZERO, s := 1.0) -> void:
	var b := Basis.from_euler(rot_deg * (PI / 180.0)).scaled(Vector3.ONE * s)
	p.append([m, Transform3D(b, pos), c])


static func _part_xf(p: Array, m: PrimitiveMesh, xf: Transform3D, c: Color) -> void:
	p.append([m, xf, c])


# Cylinder of radius r from a to b (barrels, masts, antennas, legs).
static func _rod(p: Array, a: Vector3, b: Vector3, r: float, c: Color) -> void:
	var dir := (b - a).normalized()
	var basis := Basis(Quaternion(Vector3.UP, dir)) if dir.cross(Vector3.UP).length() > 0.001 else Basis()
	p.append([_cyl(r, r, a.distance_to(b), 6), Transform3D(basis, (a + b) * 0.5), c])


static func _merge(parts: Array) -> ArrayMesh:
	var v := PackedVector3Array()
	var n := PackedVector3Array()
	var c := PackedColorArray()
	var idx := PackedInt32Array()
	for part in parts:
		var a: Array = part[0].get_mesh_arrays()
		var xf: Transform3D = Transform3D(Basis.from_scale(Vector3.ONE * SCALE), Vector3.ZERO) * part[1]
		var base := v.size()
		for x: Vector3 in a[Mesh.ARRAY_VERTEX]:
			v.append(xf * x)
			c.append(part[2])
		for x: Vector3 in a[Mesh.ARRAY_NORMAL]:
			n.append((xf.basis * x).normalized())
		for i: int in a[Mesh.ARRAY_INDEX]:
			idx.append(base + i)
	var arrays := []
	arrays.resize(Mesh.ARRAY_MAX)
	arrays[Mesh.ARRAY_VERTEX] = v
	arrays[Mesh.ARRAY_NORMAL] = n
	arrays[Mesh.ARRAY_COLOR] = c
	arrays[Mesh.ARRAY_INDEX] = idx
	var mesh := ArrayMesh.new()
	mesh.add_surface_from_arrays(Mesh.PRIMITIVE_TRIANGLES, arrays)
	return mesh
