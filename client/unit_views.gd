# SPDX-License-Identifier: AGPL-3.0-or-later
# 3D unit display (display only): models, base rings, labels, health bars, damage smoke, wrecks,
# and the planning overlay (move chevrons, ghost silhouettes at the previewed end point, stop marks,
# fire mission arcs). Reads main's state every frame; never changes it.
extends Node3D

const Models := preload("res://models.gd")
const Hud := preload("res://hud.gd")

const BAR_SHADER := """
shader_type spatial;
render_mode unshaded, depth_test_disabled, cull_disabled, shadows_disabled;
instance uniform float fill = 1.0;
void vertex() {
	MODELVIEW_MATRIX = VIEW_MATRIX * mat4(INV_VIEW_MATRIX[0], INV_VIEW_MATRIX[1], INV_VIEW_MATRIX[2], MODEL_MATRIX[3]);
	VERTEX *= length(MODEL_MATRIX[0].xyz);
}
void fragment() {
	vec3 col = fill > 0.5 ? vec3(0.25, 0.95, 0.25) : (fill > 0.25 ? vec3(1.0, 0.8, 0.1) : vec3(1.0, 0.2, 0.1));
	bool edge = UV.y < 0.2 || UV.y > 0.8 || UV.x < 0.02 || UV.x > 0.98;
	ALBEDO = edge ? vec3(0.0) : (UV.x < fill ? col : vec3(0.12));
}
"""
const WRECK_SMOKE_S := 45.0
const LABEL_FADE_M := 12000.0
const UNKNOWN := Color(0.75, 0.75, 0.75)
const GLOW_M := 10.0  # emitter glow radius

var main  # main.gd
var fx  # fx.gd
var views := {}  # id -> {root, body, extra, ring, bar, label, mark, smoke, key}
var wrecks := {}  # id -> {root, smoke, born}
var ghosts: Array = []  # pooled MeshInstance3D
var plan := MeshInstance3D.new()
var body_mat := StandardMaterial3D.new()
var wreck_mat := StandardMaterial3D.new()
var bar_mat := ShaderMaterial.new()
var ring_mats := {}  # team index / "sel" / "unknown" -> material
var ghost_mats := {}
var ring_mesh := TorusMesh.new()
var bar_mesh := QuadMesh.new()


func _init(owner_main, effects) -> void:
	main = owner_main
	fx = effects


func _ready() -> void:
	body_mat.vertex_color_use_as_albedo = true
	body_mat.vertex_color_is_srgb = true
	body_mat.roughness = 0.75
	wreck_mat.albedo_color = Color(0.07, 0.065, 0.06)
	wreck_mat.roughness = 1.0
	var sh := Shader.new()
	sh.code = BAR_SHADER
	bar_mat.shader = sh
	bar_mesh.size = Vector2(26, 3.2)
	bar_mesh.material = bar_mat
	ring_mesh.inner_radius = 17.0
	ring_mesh.outer_radius = 21.0
	ring_mesh.rings = 24
	ring_mesh.ring_segments = 4
	for key in [0, 1, "sel", "unknown"]:
		var c: Color = Color.YELLOW if key is String and key == "sel" else (UNKNOWN if key is String else main.TEAM_COLORS[key])
		ring_mats[key] = _flat(c, false)
		if key is int:
			ghost_mats[key] = _flat(Color(c.lightened(0.3), 0.35), true)
	plan.mesh = ImmediateMesh.new()
	plan.material_override = _flat(Color.WHITE, true)
	plan.material_override.vertex_color_use_as_albedo = true
	plan.material_override.vertex_color_is_srgb = true
	plan.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	add_child(plan)


## Zoom factor: models, rings and bars grow when the camera is far so they stay readable.
func zoom() -> float:
	return clampf(main.cam_dist / 400.0, 1.0, 10.0)


func team_color(owner: int) -> Color:
	return main.TEAM_COLORS[owner] if owner >= 0 and owner < 2 else UNKNOWN


# ---------------------------------------------------------------- per frame

func sync() -> void:
	var k := zoom()
	var seen := {}
	for id in main.units:
		var u: Dictionary = main.units[id]
		if u.state == "destroyed":
			if not wrecks.has(id) and u.kind == "exact":
				add_wreck(id)
			continue
		if not (u.owner == main.player_id or (u.quality >= 2 and u.kind == "exact")):
			continue  # approximate contacts: drawn by main as faded slabs / bearings
		seen[id] = true
		_update_view(id, u, k)
	for id in views.keys():
		if not seen.has(id):
			views[id].root.queue_free()
			views.erase(id)
	var now := Time.get_ticks_msec() / 1000.0
	for id in wrecks:
		var w: Dictionary = wrecks[id]
		w.root.scale = Vector3.ONE * k
		if w.smoke != null and now - w.born > WRECK_SMOKE_S:
			w.smoke.emitting = false
	_draw_plan(k)


func _update_view(id: int, u: Dictionary, k: float) -> void:
	var key := "%s|%d" % [u.type, u.owner]
	var v: Dictionary = views.get(id, {})
	if v.get("key") != key:
		if not v.is_empty():
			v.root.queue_free()
		v = _make_view(u, key)
		views[id] = v
	var lift: float = Models.lift(u.type)
	v.root.position = main.to_world(u.pos, lift)
	v.root.scale = Vector3.ONE * k
	v.ring.position.y = 4.0 - lift / k
	v.body.rotation.y = -u.heading - PI / 2.0
	var sel: bool = id in main.selection
	v.ring.material_override = ring_mats["sel"] if sel else ring_mats.get(u.owner, ring_mats["unknown"])
	v.ring.scale = Vector3(1.15, 1, 1.15) if sel else Vector3.ONE
	var frac := _health(u)
	v.bar.set_instance_shader_parameter("fill", frac)
	if v.glow != null:  # emitting radar (cyan, rotating head) / jammer (magenta)
		var on: bool = u.get("emitting", false)
		v.glow.visible = on
		if on:
			var jam: bool = v.extra == null
			var pulse := 0.5 + 0.5 * sin(Time.get_ticks_msec() / (120.0 if jam else 150.0))
			var c := Color(1.0, 0.3, 1.0) if jam else Color(0.3, 1.0, 1.0)
			v.glow.set_instance_shader_parameter("tint", Color(c, 0.3 + 0.5 * pulse))
			v.glow.scale = Vector3.ONE * GLOW_M * (0.7 + 0.6 * pulse if jam else 0.8 + 0.4 * pulse)
			if not jam:
				v.extra.rotation.y += get_process_delta_time() * 2.5
	var damaged: bool = u.state == "damaged"
	if damaged and v.smoke == null:
		v.smoke = fx.smoke_emitter(false)
		v.smoke.position = Vector3(0, 10, 0)
		v.root.add_child(v.smoke)
	elif not damaged and v.smoke != null:
		v.smoke.queue_free()
		v.smoke = null
	for l in [v.label, v.mark, v.stop]:
		l.scale = Vector3.ONE / k  # fixed-size labels must not grow with the zoomed model
	var text := "%s %d" % [Hud.short_name(u.type), id]
	if v.label.text != text:  # avoid re-shaping text every frame
		v.label.text = text
	var marks := _order_marks(id) if u.owner == main.player_id and not main.playing else ""
	if v.mark.text != marks:
		v.mark.text = marks
	v.stop.visible = u.owner == main.player_id and not main.playing and main.orders.has_stop(id)


func _make_view(u: Dictionary, key: String) -> Dictionary:
	var c := team_color(u.owner)
	var root := Node3D.new()
	add_child(root)
	var body := MeshInstance3D.new()
	body.mesh = Models.body(u.type, c)
	body.material_override = body_mat
	root.add_child(body)
	var v := {"root": root, "body": body, "extra": null, "glow": null, "smoke": null, "key": key}
	var head = Models.extra(u.type, c)
	if head != null:
		v.extra = MeshInstance3D.new()
		v.extra.mesh = head
		v.extra.material_override = body_mat
		v.extra.position = Models.DISH_PIVOT
		body.add_child(v.extra)
	if u.type in ["gsr", "jammer"]:  # emitting glow
		v.glow = MeshInstance3D.new()
		v.glow.mesh = fx.meshes["glow"]
		v.glow.material_override = fx.add_mat
		v.glow.position = Models.DISH_PIVOT + Vector3(0, 0 if u.type == "gsr" else -4, 0)
		v.glow.visible = false
		body.add_child(v.glow)
	v.ring = MeshInstance3D.new()
	v.ring.mesh = ring_mesh
	v.ring.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	root.add_child(v.ring)
	v.bar = MeshInstance3D.new()
	v.bar.mesh = bar_mesh
	v.bar.position.y = 48.0
	v.bar.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	root.add_child(v.bar)
	v.label = _label(c.lightened(0.45), 26)
	v.label.position.y = 60.0
	root.add_child(v.label)
	v.mark = _label(Color(1.0, 0.75, 0.3), 22)
	v.mark.position.y = 75.0
	root.add_child(v.mark)
	v.stop = _label(Color(1.0, 0.15, 0.1), 64)
	v.stop.text = "X"
	v.stop.position.y = 20.0
	v.stop.visible = false
	root.add_child(v.stop)
	return v


func _label(c: Color, size: int) -> Label3D:
	var l := Label3D.new()
	l.billboard = BaseMaterial3D.BILLBOARD_ENABLED
	l.fixed_size = true
	l.pixel_size = 0.0011
	l.font_size = size
	l.outline_size = 8
	l.no_depth_test = true
	l.modulate = c
	l.outline_modulate = Color(0, 0, 0, 0.85)
	l.visibility_range_end = LABEL_FADE_M
	l.visibility_range_end_margin = 3000.0
	l.visibility_range_fade_mode = GeometryInstance3D.VISIBILITY_RANGE_FADE_SELF
	return l


# Health as 0..1: hp against the type's maximum when known (own units), else from the state.
func _health(u: Dictionary) -> float:
	if u.get("hp") != null:
		return clampf(float(u.hp) / main.max_hp(u.type), 0.0, 1.0)
	return 0.5 if u.state == "damaged" else 1.0


# Pending stance / emitter / fire mission orders as a short text above the unit.
func _order_marks(id: int) -> String:
	var out: Array = []
	var st = main.orders.pending_stance(id)
	if st != null:
		out.append("[%s]" % Hud.STANCE_NAMES.get(st, st))
	var em = main.orders.pending_emitter(id)
	if em != null:
		out.append("[Yayın %s]" % ("açık" if em else "kapalı"))
	if main.orders.pending_mission(id) != null:
		out.append("[Atış]")
	return " ".join(out)


func add_wreck(id: int) -> void:
	var u: Dictionary = main.units[id]
	if wrecks.has(id) or u.get("kind", "exact") != "exact":
		return
	var root := Node3D.new()
	root.position = main.to_world(u.pos)
	add_child(root)
	var body := MeshInstance3D.new()
	body.mesh = Models.body(u.type if Models.lift(u.type) == 0.0 else "", UNKNOWN)
	body.material_override = wreck_mat
	body.rotation = Vector3(0.12, -u.heading - PI / 2.0 + 0.3, 0.08)
	body.position.y = -2.0
	root.add_child(body)
	var smoke: GPUParticles3D = fx.smoke_emitter(true)
	smoke.position = Vector3(0, 8, 0)
	root.add_child(smoke)
	wrecks[id] = {"root": root, "smoke": smoke, "born": Time.get_ticks_msec() / 1000.0}


# ---------------------------------------------------------------- planning overlay

func _draw_plan(k: float) -> void:
	var im: ImmediateMesh = plan.mesh
	im.clear_surfaces()
	var used := 0
	if not main.playing and main.turn > 0:
		var tris := PackedVector3Array()
		var cols := PackedColorArray()
		var team := team_color(main.player_id).lightened(0.35)
		for id in main.previews:
			var path: Array = main.previews[id]
			if path.is_empty() or not main.units.has(id):
				continue
			var u: Dictionary = main.units[id]
			var pts: Array = [u.pos] + path
			_chevrons(tris, cols, pts, k, Color(team, 0.9))
			var end: Vector2 = pts[-1]
			var dir: Vector2 = end - pts[-2]
			_ghost(used, u, end, atan2(dir.y, dir.x) if dir.length() > 1.0 else u.heading, k)
			used += 1
		for o in main.orders.missions():
			var t = o.target.pos if o.target.kind == "point" else main.units.get(int(o.target.unit), {}).get("pos")
			if t == null:
				continue
			_reticle(tris, cols, t, k)
			for id in o.units:
				if main.units.has(id):
					_arc(tris, cols, main.units[id].pos, t, k)
		if not tris.is_empty():
			im.surface_begin(Mesh.PRIMITIVE_TRIANGLES)
			for i in tris.size():
				im.surface_set_color(cols[i])
				im.surface_add_vertex(tris[i])
			im.surface_end()
	for i in range(used, ghosts.size()):
		ghosts[i].visible = false


func _ghost(i: int, u: Dictionary, at: Vector2, heading: float, k: float) -> void:
	if i >= ghosts.size():
		var g := MeshInstance3D.new()
		g.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
		add_child(g)
		ghosts.append(g)
	var g: MeshInstance3D = ghosts[i]
	g.visible = true
	g.mesh = Models.body(u.type, team_color(u.owner))
	g.material_override = ghost_mats.get(u.owner, ghost_mats[0])
	g.transform = Transform3D(Basis.from_euler(Vector3(0, -heading - PI / 2.0, 0)).scaled(Vector3.ONE * k),
		main.to_world(at, Models.lift(u.type)))


# Chevrons every few sizes along the path, a big arrowhead at the end.
func _chevrons(tris: PackedVector3Array, cols: PackedColorArray, pts: Array, k: float, c: Color) -> void:
	var size := 14.0 * k
	var step := size * 2.6
	var carry := step * 0.8
	for i in range(pts.size() - 1):
		var a: Vector2 = pts[i]
		var b: Vector2 = pts[i + 1]
		var len := a.distance_to(b)
		if len < 0.01:
			continue
		var dir := (b - a) / len
		var s := carry
		while s < len - size * 0.5 or (i < pts.size() - 2 and s < len):
			_chevron(tris, cols, a + dir * s, dir, size, c, false)
			s += step
		carry = s - len
	var last_dir: Vector2 = (pts[-1] - pts[-2]).normalized()
	_chevron(tris, cols, pts[-1] - last_dir * size, last_dir, size * 1.8, c, true)


# V shape (or a filled arrowhead) at p pointing along dir, flat above the ground.
func _chevron(tris: PackedVector3Array, cols: PackedColorArray, p: Vector2, dir: Vector2, s: float, c: Color, head: bool) -> void:
	var side := dir.orthogonal()
	var h: float = main.ground(p) + 6.0 * s / 14.0
	var w := func(q: Vector2) -> Vector3: return Vector3(q.x, h, q.y)
	var tip: Vector2 = p + dir * s * 0.5
	if head:
		tris.append_array([w.call(tip), w.call(p - dir * s * 0.5 + side * s * 0.6), w.call(p - dir * s * 0.5 - side * s * 0.6)])
		for k in 3:
			cols.append(c)
		return
	var t := s * 0.28  # arm thickness
	for sgn in [1.0, -1.0]:
		var back: Vector2 = p - dir * s * 0.5 + side * sgn * s * 0.55
		var n: Vector2 = (tip - back).orthogonal().normalized() * t * 0.5
		tris.append_array([w.call(tip + n), w.call(tip - n), w.call(back + n), w.call(back + n), w.call(tip - n), w.call(back - n)])
		for k in 6:
			cols.append(c)


# Ballistic-looking arc (display only) from the shooter to the target as a flat ribbon.
func _arc(tris: PackedVector3Array, cols: PackedColorArray, a: Vector2, b: Vector2, k: float) -> void:
	var pa: Vector3 = main.to_world(a, 20)
	var pb: Vector3 = main.to_world(b, 20)
	var peak := a.distance_to(b) * 0.12
	var side := Vector3((b - a).orthogonal().normalized().x, 0, (b - a).orthogonal().normalized().y) * 1.2 * k
	var c := Color(1.0, 0.45, 0.2, 0.85)
	var prev := pa
	for i in range(1, 33):
		var f := i / 32.0
		var q := pa.lerp(pb, f) + Vector3(0, peak * 4.0 * f * (1.0 - f), 0)
		tris.append_array([prev + side, prev - side, q + side, q + side, prev - side, q - side])
		for n in 6:
			cols.append(c)
		prev = q


func _reticle(tris: PackedVector3Array, cols: PackedColorArray, p: Vector2, k: float) -> void:
	var r := maxf(main.MISSION_MARK_M, 40.0 * k)
	var t := 2.5 * k
	var c := Color(1.0, 0.25, 0.15, 0.9)
	var h: float = main.ground(p) + 10.0
	var n := 32
	for i in n:
		var a := TAU * i / n
		var b := TAU * (i + 1) / n
		var d1 := Vector2(cos(a), sin(a))
		var d2 := Vector2(cos(b), sin(b))
		var q := [p + d1 * (r - t), p + d1 * (r + t), p + d2 * (r - t), p + d2 * (r + t)]
		for v in [q[0], q[1], q[2], q[2], q[1], q[3]]:
			tris.append(Vector3(v.x, h, v.y))
			cols.append(c)
	for d in [Vector2.RIGHT, Vector2.DOWN, Vector2.LEFT, Vector2.UP]:  # cross hair ticks
		var s: Vector2 = d.orthogonal() * t
		var a: Vector2 = p + d * r * 0.4
		var b: Vector2 = p + d * r * 1.3
		for v in [a + s, a - s, b + s, b + s, a - s, b - s]:
			tris.append(Vector3(v.x, h, v.y))
			cols.append(c)


func _flat(c: Color, alpha: bool) -> StandardMaterial3D:
	var m := StandardMaterial3D.new()
	m.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	m.albedo_color = c
	m.cull_mode = BaseMaterial3D.CULL_DISABLED
	if alpha:
		m.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
	return m
