# SPDX-License-Identifier: AGPL-3.0-or-later
# Client: display, input and UI only. Every game rule is computed by the server.
extends Node3D

const MAP_DIR := "../maps/hasandag"
const TICKS_PER_SECOND := 10.0
const RESOLUTION_TICKS := 200
# Display sizes in metres (exaggerated so units are visible on a 40 km map).
const BOX_SIZE := {"infantry": Vector3(40, 30, 40), "mbt": Vector3(60, 40, 100)}
const TEAM_COLORS := [Color(0.25, 0.55, 1.0), Color(1.0, 0.3, 0.25)]
const DEAD_COLOR := Color(0.2, 0.2, 0.2)

var net := Net.new()
var terrain: Terrain3D
var camera := Camera3D.new()
var cam_target := Vector3(20000, 0, 24000)
var cam_yaw := 0.0
var cam_dist := 9000.0

var solid := MultiMeshInstance3D.new()
var faded := MultiMeshInstance3D.new()
var lines := MeshInstance3D.new()

var player_id := -1
var turn := 0
var deadline := 0.0
var kbg_budget := 0
# id -> {owner, type, pos: Vector2, heading, state, quality, area: bool}
var units := {}
var selected := -1
# id -> Array[Vector2] waypoints; id -> Array[Vector2] previewed path
var orders := {}
var previews := {}

# Resolution playback
var playing := false
var play_time := 0.0
var events: Array = []
var event_index := 0
var keyframes := {}  # id -> Array of [tick, Vector2]
var shots: Array = []  # [from Vector3, to Vector3, time_left]
var pending_turn_start = null

@onready var ui := _build_ui()


func _ready() -> void:
	_load_terrain()
	camera.far = 80000.0
	camera.near = 2.0
	add_child(camera)
	var sun := DirectionalLight3D.new()
	sun.rotation_degrees = Vector3(-50, -30, 0)
	sun.shadow_enabled = false
	add_child(sun)
	var env := WorldEnvironment.new()
	env.environment = Environment.new()
	env.environment.background_mode = Environment.BG_COLOR
	env.environment.background_color = Color(0.55, 0.7, 0.85)
	env.environment.ambient_light_color = Color(0.6, 0.6, 0.6)
	env.environment.ambient_light_source = Environment.AMBIENT_SOURCE_COLOR
	add_child(env)
	for mmi in [solid, faded]:
		var mm := MultiMesh.new()
		mm.transform_format = MultiMesh.TRANSFORM_3D
		mm.use_colors = true
		mm.mesh = BoxMesh.new()
		var mat := StandardMaterial3D.new()
		mat.vertex_color_use_as_albedo = true
		if mmi == faded:
			mat.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
		mat.shading_mode = BaseMaterial3D.SHADING_MODE_PER_PIXEL
		mm.mesh.surface_set_material(0, mat)
		mmi.multimesh = mm
		add_child(mmi)
	lines.mesh = ImmediateMesh.new()
	var line_mat := StandardMaterial3D.new()
	line_mat.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	line_mat.vertex_color_use_as_albedo = true
	lines.material_override = line_mat
	add_child(lines)
	net.opened.connect(_on_open)
	net.closed.connect(func(): _status("Bağlantı koptu. Aynı isimle tekrar bağlanabilirsiniz."))
	net.message.connect(_on_message)


func _process(delta: float) -> void:
	net.poll()
	_move_camera(delta)
	if playing:
		_advance_playback(delta)
	elif deadline > 0.0:
		deadline = max(0.0, deadline - delta)
	_update_hud()
	_draw_units()
	_draw_lines(delta)


# ---------------------------------------------------------------- terrain

func _load_terrain() -> void:
	var dir := ProjectSettings.globalize_path("res://").path_join(MAP_DIR)
	var meta := _read_toml(dir.path_join("map.toml"))
	if not meta.has("terrain_r16"):
		push_warning("map not found in %s; flat ground" % dir)
		var ground := MeshInstance3D.new()
		var plane := PlaneMesh.new()
		plane.size = Vector2(40000, 40000)
		ground.mesh = plane
		ground.position = Vector3(20000, 0, 20000)
		add_child(ground)
		return
	var r: Dictionary = meta["terrain_r16"]
	var img := Terrain3DUtil.load_image(dir.path_join(r["file"]), ResourceLoader.CACHE_MODE_IGNORE,
		Vector2(r["min_height_m"], r["max_height_m"]), Vector2i(int(r["width"]), int(r["height"])))
	terrain = Terrain3D.new()
	terrain.region_size = 1024
	terrain.vertex_spacing = float(r["pixel_size_x_m"])
	add_child(terrain)
	terrain.material.show_checkered = false
	terrain.data.import_images([img, null, null], Vector3.ZERO, 0.0, 1.0)


func ground(p: Vector2) -> float:
	if terrain == null:
		return 0.0
	var h: float = terrain.data.get_height(Vector3(p.x, 0, p.y))
	return 0.0 if is_nan(h) else h


func to_world(p: Vector2, lift := 0.0) -> Vector3:
	return Vector3(p.x, ground(p) + lift, p.y)


# Minimal reader for map.toml: key = value lines and [sections].
func _read_toml(path: String) -> Dictionary:
	var out := {}
	var section := out
	var f := FileAccess.open(path, FileAccess.READ)
	if f == null:
		return out
	while not f.eof_reached():
		var line := f.get_line().strip_edges()
		if line.begins_with("#") or line.is_empty():
			continue
		if line.begins_with("["):
			section = {}
			out[line.trim_prefix("[").trim_suffix("]")] = section
			continue
		var kv := line.split("=", false, 1)
		if kv.size() != 2:
			continue
		var value := kv[1].split("#")[0].strip_edges() if not kv[1].strip_edges().begins_with("\"") else kv[1].strip_edges().split("\"")[1]
		section[kv[0].strip_edges()] = float(value) if value.is_valid_float() else value
	return out


# ---------------------------------------------------------------- network

func _on_open() -> void:
	net.send({"type": "hello", "protocol_version": Net.PROTOCOL_VERSION, "player_name": ui.name_box.text})
	_status("Bağlandı, rakip bekleniyor...")


func _on_message(m: Dictionary) -> void:
	match m.get("type"):
		"welcome":
			player_id = int(m.player_id)
			_status("Oyuncu %d, harita %s" % [player_id + 1, m.map_id])
		"turn_start":
			if playing:
				pending_turn_start = m
			else:
				_apply_turn_start(m)
		"resolution":
			_start_playback(m.events)
		"preview_result":
			if m.reason == null:
				previews[int(m.unit)] = m.path.map(func(p): return Vector2(p.x, p.y))
			else:
				_status("Rota yok: %s" % m.reason)
		"order_rejected":
			_status("Emir reddedildi: %s" % m.reason)
		"error":
			_status("Sunucu hatası (%s): %s" % [m.code, m.message])


func _apply_turn_start(m: Dictionary) -> void:
	turn = int(m.turn)
	deadline = float(m.deadline_secs)
	kbg_budget = int(m.view.kbg_budget)
	units.clear()
	orders.clear()
	previews.clear()
	for u in m.view.own_units:
		units[int(u.id)] = {"owner": player_id, "type": u.type_id, "pos": Vector2(u.pos.x, u.pos.y),
			"heading": u.heading_rad, "state": u.state, "quality": 3, "area": false}
	for c in m.view.contacts:
		var p := _position(c.position)
		units[int(c.unit)] = {"owner": 1 - player_id, "type": c.type_id if c.type_id != null else "", "pos": p[0],
			"heading": 0.0, "state": "healthy", "quality": int(c.quality), "area": p[1]}
	ui.ready_button.disabled = false
	_status("Tur %d: planlama. Sol tık birim seç, sağ tık rota noktası." % turn)


func _position(pos: Dictionary) -> Array:
	if pos.kind == "exact":
		return [Vector2(pos.pos.x, pos.pos.y), false]
	return [Vector2(pos.center.x, pos.center.y), true]


func _submit_orders() -> void:
	var list := []
	for id in orders:
		list.append({"type": "move", "unit": id,
			"waypoints": orders[id].map(func(p): return {"x": p.x, "y": p.y})})
	net.send({"type": "submit_orders", "turn": turn, "orders": list})


# ---------------------------------------------------------------- playback

func _start_playback(evs: Array) -> void:
	events = evs
	event_index = 0
	play_time = 0.0
	playing = true
	keyframes.clear()
	for id in units:
		keyframes[id] = [[0.0, units[id].pos]]
	for e in events:
		if e.type == "unit_moved" or e.type == "track_gained":
			var id := int(e.unit)
			if not keyframes.has(id):
				keyframes[id] = []
			keyframes[id].append([float(e.tick), _position(e.position)[0]])
	selected = -1
	previews.clear()
	ui.ready_button.disabled = true
	_status("Tur %d: çözüm oynatılıyor." % turn)


func _advance_playback(delta: float) -> void:
	play_time += delta
	var tick := play_time * TICKS_PER_SECOND
	while event_index < events.size() and float(events[event_index].tick) <= tick:
		_apply_event(events[event_index])
		event_index += 1
	for id in units:
		var k: Array = keyframes.get(id, [])
		units[id].pos = _interpolate(k, tick, units[id].pos)
	if tick >= RESOLUTION_TICKS:
		playing = false
		if pending_turn_start != null:
			_apply_turn_start(pending_turn_start)
			pending_turn_start = null


func _interpolate(k: Array, tick: float, fallback: Vector2) -> Vector2:
	if k.is_empty() or tick < k[0][0]:
		return fallback
	for i in range(k.size() - 1):
		if tick <= k[i + 1][0]:
			var span: float = max(k[i + 1][0] - k[i][0], 0.001)
			return k[i][1].lerp(k[i + 1][1], (tick - k[i][0]) / span)
	return k[-1][1]


func _apply_event(e: Dictionary) -> void:
	match e.type:
		"unit_moved":
			var id := int(e.unit)
			if units.has(id):
				units[id].area = _position(e.position)[1]
				if e.heading_rad != null:
					units[id].heading = e.heading_rad
		"track_gained":
			var p := _position(e.position)
			var id := int(e.unit)
			if not units.has(id):
				units[id] = {"owner": 1 - player_id, "type": "", "pos": p[0], "heading": 0.0, "state": "healthy"}
			units[id].quality = int(e.quality)
			units[id].area = p[1]
		"track_lost":
			var id := int(e.unit)
			if units.has(id):
				units[id].quality = 0
				units[id].area = true
				keyframes.erase(id)
		"unit_damaged":
			if units.has(int(e.unit)):
				units[int(e.unit)].state = e.state
		"unit_destroyed":
			if units.has(int(e.unit)):
				units[int(e.unit)].state = "destroyed"
		"shot_fired":
			if e.shooter != null and e.target != null and units.has(int(e.shooter)) and units.has(int(e.target)):
				shots.append([to_world(units[int(e.shooter)].pos, 20), to_world(units[int(e.target)].pos, 20), 0.4])


# ---------------------------------------------------------------- drawing

func _draw_units() -> void:
	var s: Array = []
	var f: Array = []
	for id in units:
		var u: Dictionary = units[id]
		var size: Vector3 = BOX_SIZE.get(u.type, Vector3(50, 30, 50))
		var color: Color = TEAM_COLORS[u.owner] if u.owner >= 0 and u.owner < 2 else Color.WHITE
		if u.state == "destroyed":
			color = DEAD_COLOR
		elif u.state == "damaged":
			color = color.darkened(0.4)
		if id == selected:
			color = Color.YELLOW
		var basis := Basis.looking_at(Vector3(cos(u.heading), 0, sin(u.heading))).scaled(size)
		var xf := Transform3D(basis, to_world(u.pos, size.y * 0.5))
		if u.owner == player_id or u.quality >= 2:
			s.append([xf, color])
		elif u.area:
			# Approximate contact: a large faded slab over the area.
			xf = Transform3D(Basis.from_scale(Vector3(500, 10, 500)), to_world(u.pos, 5))
			f.append([xf, Color(color, 0.25 if u.quality > 0 else 0.12)])
		else:
			f.append([xf, Color(color, 0.3)])
	_fill(solid.multimesh, s)
	_fill(faded.multimesh, f)


func _fill(mm: MultiMesh, items: Array) -> void:
	if mm.instance_count != items.size():
		mm.instance_count = items.size()
	for i in items.size():
		mm.set_instance_transform(i, items[i][0])
		mm.set_instance_color(i, items[i][1])


func _draw_lines(delta: float) -> void:
	var im: ImmediateMesh = lines.mesh
	im.clear_surfaces()
	var segs: Array = []
	for id in previews:
		var pts: Array = [units[id].pos] + previews[id] if units.has(id) else previews[id]
		for i in range(pts.size() - 1):
			segs.append([to_world(pts[i], 15), to_world(pts[i + 1], 15), Color.YELLOW])
	for shot in shots:
		segs.append([shot[0], shot[1], Color.ORANGE])
		shot[2] -= delta
	shots = shots.filter(func(sh): return sh[2] > 0.0)
	if segs.is_empty():
		return
	im.surface_begin(Mesh.PRIMITIVE_LINES)
	for seg in segs:
		im.surface_set_color(seg[2])
		im.surface_add_vertex(seg[0])
		im.surface_add_vertex(seg[1])
	im.surface_end()


# ---------------------------------------------------------------- input & camera

func _unhandled_input(event: InputEvent) -> void:
	if event is InputEventMouseButton and event.pressed:
		match event.button_index:
			MOUSE_BUTTON_WHEEL_UP:
				cam_dist = max(300.0, cam_dist * 0.85)
			MOUSE_BUTTON_WHEEL_DOWN:
				cam_dist = min(40000.0, cam_dist / 0.85)
			MOUSE_BUTTON_LEFT:
				if not playing:
					selected = _pick_own_unit(event.position)
			MOUSE_BUTTON_RIGHT:
				if not playing and selected >= 0:
					_add_waypoint(event.position)


func _pick_own_unit(screen: Vector2) -> int:
	var best := -1
	var best_d := 30.0
	for id in units:
		if units[id].owner != player_id or units[id].state == "destroyed":
			continue
		var w := to_world(units[id].pos, 20)
		if camera.is_position_behind(w):
			continue
		var d := camera.unproject_position(w).distance_to(screen)
		if d < best_d:
			best_d = d
			best = id
	return best


func _add_waypoint(screen: Vector2) -> void:
	var from := camera.project_ray_origin(screen)
	var dir := camera.project_ray_normal(screen)
	var hit: Vector3
	if terrain != null:
		hit = terrain.get_intersection(from, dir)
	else:
		hit = from + dir * (-from.y / dir.y) if dir.y < 0.0 else Vector3(INF, 0, 0)
	if not is_finite(hit.x) or hit.z > 3.0e38:
		return
	if not orders.has(selected):
		orders[selected] = []
	orders[selected].append(Vector2(hit.x, hit.z))
	var wps: Array = orders[selected].map(func(p): return {"x": p.x, "y": p.y})
	net.send({"type": "request_preview", "unit": selected, "waypoints": wps})
	_submit_orders()


func _move_camera(delta: float) -> void:
	var pan := Vector3.ZERO
	if Input.is_key_pressed(KEY_W) or Input.is_key_pressed(KEY_UP): pan.z -= 1
	if Input.is_key_pressed(KEY_S) or Input.is_key_pressed(KEY_DOWN): pan.z += 1
	if Input.is_key_pressed(KEY_A) or Input.is_key_pressed(KEY_LEFT): pan.x -= 1
	if Input.is_key_pressed(KEY_D) or Input.is_key_pressed(KEY_RIGHT): pan.x += 1
	if Input.is_key_pressed(KEY_Q): cam_yaw += delta
	if Input.is_key_pressed(KEY_E): cam_yaw -= delta
	cam_target += pan.rotated(Vector3.UP, cam_yaw) * cam_dist * delta
	cam_target.y = ground(Vector2(cam_target.x, cam_target.z))
	var offset := Vector3(0, cam_dist * 0.8, cam_dist * 0.6).rotated(Vector3.UP, cam_yaw)
	camera.look_at_from_position(cam_target + offset, cam_target)


# ---------------------------------------------------------------- UI

func _build_ui() -> Dictionary:
	var layer := CanvasLayer.new()
	add_child(layer)
	var box := HBoxContainer.new()
	box.position = Vector2(10, 10)
	layer.add_child(box)
	var addr := LineEdit.new()
	addr.text = "127.0.0.1:7777"
	addr.custom_minimum_size.x = 180
	var name_box := LineEdit.new()
	name_box.text = "oyuncu"
	name_box.custom_minimum_size.x = 120
	var connect_button := Button.new()
	connect_button.text = "Bağlan"
	connect_button.pressed.connect(func():
		if net.connect_to(addr.text) != OK:
			_status("Adres geçersiz"))
	var ready_button := Button.new()
	ready_button.text = "Hazır"
	ready_button.disabled = true
	ready_button.pressed.connect(func():
		_submit_orders()
		net.send({"type": "ready"})
		ready_button.disabled = true
		_status("Hazır. Rakip bekleniyor..."))
	var hud := Label.new()
	for c in [addr, name_box, connect_button, ready_button, hud]:
		box.add_child(c)
	var status := Label.new()
	status.position = Vector2(10, 50)
	layer.add_child(status)
	var fps := Label.new()
	fps.position = Vector2(10, 75)
	layer.add_child(fps)
	return {"name_box": name_box, "ready_button": ready_button, "hud": hud, "status": status, "fps": fps}


func _update_hud() -> void:
	if playing:
		ui.hud.text = "  Tur %d  çözüm %.1f / 20 sn" % [turn, play_time]
	elif turn > 0:
		ui.hud.text = "  Tur %d  kalan %d sn  KBG: %d emir / bütçe %d" % [turn, ceil(deadline), orders.size(), kbg_budget]
	ui.fps.text = "FPS %d" % Engine.get_frames_per_second()


func _status(text: String) -> void:
	ui.status.text = text
