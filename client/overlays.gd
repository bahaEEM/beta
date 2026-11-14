# SPDX-License-Identifier: AGPL-3.0-or-later
# Static-ish map overlays draped on the terrain: roads (built once), region borders and zones (recoloured on change).
extends Node3D

# Road class -> [ribbon width m, colour]. 3 = main road, 1 = track.
const ROAD_STYLE := {
	3: [36.0, Color(1.0, 0.85, 0.35)],
	2: [20.0, Color(0.9, 0.8, 0.6, 0.85)],
	1: [10.0, Color(0.75, 0.7, 0.6, 0.55)],
}
const ROAD_LIFT := 4.0
const ROAD_STEP := 60.0  # densify long segments so ribbons follow the ground
const ZONE_LIFT := 12.0
const NEUTRAL := Color(0.6, 0.6, 0.6)
const FRONT := Color(1.0, 0.95, 0.3)  # border between regions of the two sides

var main: Node3D  # provides ground(Vector2) -> float
var zone_lines := MeshInstance3D.new()
var zone_labels := {}  # zone id -> Label3D
var borders: Array = []  # [{a, b, pts: PackedVector3Array of draped line pairs}]


func _init(owner_main: Node3D) -> void:
	main = owner_main


func _ready() -> void:
	zone_lines.mesh = ImmediateMesh.new()
	zone_lines.material_override = _material(Color.WHITE, true)
	add_child(zone_lines)


# roads_lines.json: {"lines": [{"class": 1..3, "points": [[x, y], ...]}]} in sim metres.
func build_roads(path: String) -> void:
	var f := FileAccess.open(path, FileAccess.READ)
	if f == null:
		push_warning("no roads at %s" % path)
		return
	var data = JSON.parse_string(f.get_as_text())
	if not (data is Dictionary and data.has("lines")):
		push_warning("bad roads file %s" % path)
		return
	var verts := {1: PackedVector3Array(), 2: PackedVector3Array(), 3: PackedVector3Array()}
	for line in data.lines:
		var cls := int(line["class"])
		if not verts.has(cls):
			continue
		var pts := _densify(line.points)
		_ribbon(verts[cls], pts, ROAD_STYLE[cls][0] * 0.5)
	var mesh := ArrayMesh.new()
	for cls in [1, 2, 3]:  # draw order: minor roads first
		if verts[cls].is_empty():
			continue
		var arrays := []
		arrays.resize(Mesh.ARRAY_MAX)
		arrays[Mesh.ARRAY_VERTEX] = verts[cls]
		mesh.add_surface_from_arrays(Mesh.PRIMITIVE_TRIANGLES, arrays)
		mesh.surface_set_material(mesh.get_surface_count() - 1, _material(ROAD_STYLE[cls][1], false))
	var mi := MeshInstance3D.new()
	mi.mesh = mesh
	mi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	add_child(mi)


# regions_lines.json: {"borders": [{"a": id, "b": id, "segments": [[x0, y0, x1, y1], ...]}]} in sim metres.
# Draped once here; set_zones only picks the colours.
func build_borders(path: String) -> void:
	var f := FileAccess.open(path, FileAccess.READ)
	if f == null:
		push_warning("no region borders at %s" % path)
		return
	var data = JSON.parse_string(f.get_as_text())
	if not (data is Dictionary and data.has("borders")):
		push_warning("bad region borders file %s" % path)
		return
	for b in data.borders:
		var pts := PackedVector3Array()
		for s in b.segments:
			var line := _densify([[s[0], s[1]], [s[2], s[3]]])
			for i in range(1, line.size()):
				pts.append(_drape(line[i - 1], ZONE_LIFT))
				pts.append(_drape(line[i], ZONE_LIFT))
		borders.append({"a": int(b.a), "b": int(b.b), "pts": pts})


func _densify(raw: Array) -> Array:
	var out := []
	for i in raw.size():
		var p := Vector2(raw[i][0], raw[i][1])
		if i > 0:
			var prev: Vector2 = out[-1]
			var n := int(prev.distance_to(p) / ROAD_STEP)
			for k in range(1, n):
				out.append(prev.lerp(p, float(k) / n))
		out.append(p)
	return out


# Two triangles per segment, flat ribbon of half-width hw draped ROAD_LIFT above ground.
func _ribbon(out: PackedVector3Array, pts: Array, hw: float) -> void:
	var prev_l: Vector3
	var prev_r: Vector3
	for i in pts.size():
		var a: Vector2 = pts[max(i - 1, 0)]
		var b: Vector2 = pts[min(i + 1, pts.size() - 1)]
		var side := (b - a).normalized().orthogonal() * hw
		var l := _drape(pts[i] + side, ROAD_LIFT)
		var r := _drape(pts[i] - side, ROAD_LIFT)
		if i > 0:
			out.append_array([prev_l, prev_r, l, l, prev_r, r])
		prev_l = l
		prev_r = r


func _drape(p: Vector2, lift: float) -> Vector3:
	return Vector3(p.x, main.ground(p) + lift, p.y)


# zones: view.zones from turn_start (owner may be null). Rebuilds borders, circles and labels.
# Border colours: front line between the two sides, team colour at the edge of held land, grey inside.
func set_zones(zones: Array, player_id: int, team_colors: Array) -> void:
	var im: ImmediateMesh = zone_lines.mesh
	im.clear_surfaces()
	if zones.is_empty():
		return
	im.surface_begin(Mesh.PRIMITIVE_LINES)
	var owners := {}
	for z in zones:
		owners[int(z.id)] = z.owner
	for b in borders:
		var oa = owners.get(b.a)
		var ob = owners.get(b.b)
		var line_color: Color = NEUTRAL.darkened(0.3)
		if oa != null and ob != null and int(oa) != int(ob):
			line_color = FRONT
		elif oa != null and ob != null:
			line_color = team_colors[int(oa)].darkened(0.4)
		elif oa != null or ob != null:
			line_color = team_colors[int(oa if oa != null else ob)]
		im.surface_set_color(line_color)
		for v in b.pts:
			im.surface_add_vertex(v)
	for z in zones:
		var c := Vector2(z.center.x, z.center.y)
		var r := float(z.radius_m)
		var color: Color = NEUTRAL if z.owner == null else team_colors[int(z.owner)]
		im.surface_set_color(color)
		var n := 64
		for k in n:
			for kk in [k, k + 1]:
				var a: float = TAU * kk / n
				im.surface_add_vertex(_drape(c + Vector2(cos(a), sin(a)) * r, ZONE_LIFT))
		if z.get("base") != null:  # base: a cross in the middle
			for d in [Vector2(1, 1), Vector2(1, -1)]:
				im.surface_add_vertex(_drape(c - d * r * 0.3, ZONE_LIFT))
				im.surface_add_vertex(_drape(c + d * r * 0.3, ZONE_LIFT))
		var label: Label3D = zone_labels.get(int(z.id))
		if label == null:
			label = Label3D.new()
			label.billboard = BaseMaterial3D.BILLBOARD_ENABLED
			label.fixed_size = true
			label.pixel_size = 0.0015
			label.no_depth_test = true
			add_child(label)
			zone_labels[int(z.id)] = label
		label.position = _drape(c, 150.0)
		label.modulate = color.lightened(0.3)
		var who := "tarafsız" if z.owner == null else ("bizim" if int(z.owner) == player_id else "rakip")
		if z.get("supplied") == false:  # own region whose road network has no stock (K-133)
			who += ", İKMALSİZ"
		label.text = "%s%s (%s)" % ["[Üs] " if z.get("base") != null else "", z.name_tr, who]
	im.surface_end()


func _material(color: Color, use_vertex_color: bool) -> StandardMaterial3D:
	var m := StandardMaterial3D.new()
	m.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	m.cull_mode = BaseMaterial3D.CULL_DISABLED
	m.albedo_color = color
	m.vertex_color_use_as_albedo = use_vertex_color
	if color.a < 1.0:
		m.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
	return m
