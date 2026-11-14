# SPDX-License-Identifier: AGPL-3.0-or-later
# Client: display, input and UI only. Every game rule is computed by the server.
extends Node3D

const Orders := preload("res://orders.gd")
const Hud := preload("res://hud.gd")
const Overlays := preload("res://overlays.gd")
const Fx := preload("res://fx.gd")
const UnitViews := preload("res://unit_views.gd")
const Demo := preload("res://demo.gd")
const Models := preload("res://models.gd")

const MAP_DIR := "../maps/hasandag"
const UNITS_TOML := "../data/units.toml"
const TICKS_PER_SECOND := 10.0
const RESOLUTION_TICKS := 200
const APPROX_SIZE := Vector3(50, 30, 50)  # faded box of a contact without an exact position
const SIDE_NAMES := ["Mavi", "Kırmızı"]
const SIDE_DATIVE := ["Mavi'ye", "Kırmızı'ya"]
const STANCES := ["free", "return", "hold"]
const TEAM_COLORS := [Color(0.25, 0.55, 1.0), Color(1.0, 0.3, 0.25)]
const DEAD_COLOR := Color(0.2, 0.2, 0.2)
const EMITTER_TYPES := ["gsr", "jammer", "shorad", "radar_mobile", "radar_tower", "ad_site"]
const ARTILLERY_TYPES := ["spg", "rocket"]
const MISSION_MARK_M := 150.0  # display only; the real blast radius comes with shell_impact
const BEARING_LENGTH := 40000.0
const DRAG_MIN_PX := 6.0

var net := Net.new()
var terrain: Terrain3D
var camera := Camera3D.new()
var cam_target := Vector3(20000, 0, 24000)
var cam_yaw := 0.0
var cam_dist := 9000.0

var faded := MultiMeshInstance3D.new()
var lines := MeshInstance3D.new()
var overlays := Overlays.new(self)
var ui := Hud.new()
var fx := Fx.new()
var views := UnitViews.new(self, fx)
var max_hps := {}  # type id -> hp from data/units.toml (health bar scale only)

var player_id := -1
var turn := 0
var deadline := 0.0
var kbg_budget := 0
var kbg_spent := 0
var scores: Array = []
var zones: Array = []
var game_over = null  # game_over message once received
# id -> {id, owner, type, pos: Vector2, heading, state, quality, kind: exact|area|bearing, radius, angle,
#        and for own units: hp, fuel, ammo, emitting, supply_fuel, supply_ammo, stance, connected}
var units := {}
var selection: Array = []
var groups := {}  # digit -> Array of ids (client-side control groups)
var orders := Orders.new()
var previews := {}  # id -> Array[Vector2] previewed path
var drag_start = null  # Vector2 while the left button is held
var drag_end := Vector2.ZERO
var targeting: Array = []  # artillery ids waiting for a fire mission target click (T key)

# Resolution playback
var playing := false
var play_time := 0.0
var events: Array = []
var event_index := 0
var keyframes := {}  # id -> Array of [tick, Vector2]
var logged := {}  # event log keys already written this turn (one line per shooter/target pair)
var pending_turn_start = null


func _ready() -> void:
	_load_terrain()
	add_child(overlays)
	overlays.build_roads(ProjectSettings.globalize_path("res://").path_join(MAP_DIR).path_join("roads_lines.json"))
	overlays.build_borders(ProjectSettings.globalize_path("res://").path_join(MAP_DIR).path_join("regions_lines.json"))
	add_child(ui)
	ui.connect_pressed.connect(func(addr):
		if net.connect_to(addr) != OK:
			_status("Adres geçersiz"))
	ui.ready_pressed.connect(func():
		_submit_orders()
		net.send({"type": "ready"})
		ui.ready_button.disabled = true
		_status("Hazır. Rakip bekleniyor..."))
	camera.far = 80000.0
	camera.near = 2.0
	add_child(camera)
	_environment()
	max_hps = _unit_hps(ProjectSettings.globalize_path("res://").path_join(UNITS_TOML))
	add_child(fx)
	add_child(views)
	var mm := MultiMesh.new()
	mm.transform_format = MultiMesh.TRANSFORM_3D
	mm.use_colors = true
	mm.mesh = BoxMesh.new()
	var mat := StandardMaterial3D.new()
	mat.vertex_color_use_as_albedo = true
	mat.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
	mm.mesh.surface_set_material(0, mat)
	faded.multimesh = mm
	add_child(faded)
	lines.mesh = ImmediateMesh.new()
	var line_mat := StandardMaterial3D.new()
	line_mat.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	line_mat.vertex_color_use_as_albedo = true
	line_mat.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
	lines.material_override = line_mat
	add_child(lines)
	net.opened.connect(_on_open)
	net.closed.connect(func():
		ui.set_connected(false)
		ui.banner("Bağlantı koptu.\nAynı isimle tekrar bağlanabilirsiniz.")
		_status("Bağlantı koptu."))
	net.message.connect(_on_message)
	var args := OS.get_cmdline_user_args()
	var name_at := args.find("--name")
	if name_at >= 0 and name_at + 1 < args.size():
		ui.name_box.text = args[name_at + 1]
	DisplayServer.window_set_title("beta — " + ui.name_box.text)
	if "--connect" in args:
		net.connect_to(ui.addr.text)
		_status("Bağlanılıyor...")
	var shot_at := args.find("--shot")
	if shot_at >= 0 and shot_at + 1 < args.size():
		_screenshot(args[shot_at + 1])
	if "--demo" in args:
		Demo.start(self, "--bench" in args)
	var cam_at := args.find("--cam")
	if cam_at >= 0 and cam_at + 1 < args.size():
		cam_dist = float(args[cam_at + 1])
	if "--bench" in args:
		_bench()


func _process(delta: float) -> void:
	net.poll()
	_move_camera(delta)
	if playing:
		_advance_playback(delta)
	elif deadline > 0.0:
		deadline = max(0.0, deadline - delta)
	_update_hud()
	_draw_units()
	views.sync()
	_draw_lines()


# Prints the average FPS over 10 s of camera flight across the map, then quits. With --demo the
# camera circles the demo units instead (units + effects on screen).
#   godot --path client -- --bench [--demo]
func _bench() -> void:
	print("GPU: ", RenderingServer.get_video_adapter_name())
	DisplayServer.window_set_vsync_mode(DisplayServer.VSYNC_DISABLED)  # measure headroom, not the refresh rate
	await get_tree().create_timer(2.0).timeout
	var frames := Engine.get_frames_drawn()
	var start := Time.get_ticks_msec()
	var t := 0.0
	var centre := cam_target
	var demo := not units.is_empty()
	while t < 10.0:
		await get_tree().process_frame
		t = (Time.get_ticks_msec() - start) / 1000.0
		cam_target = centre if demo else Vector3(8000 + 2400 * t, 0, 20000)
		cam_yaw = t * 0.3
	print("BENCH avg FPS %.1f (%d units)" % [(Engine.get_frames_drawn() - frames) / t, units.size()])
	get_tree().quit()


func _environment() -> void:
	var sun := DirectionalLight3D.new()
	sun.rotation_degrees = Vector3(-50, -30, 0)
	sun.light_energy = 1.2
	sun.shadow_enabled = true
	sun.directional_shadow_max_distance = 12000.0
	add_child(sun)
	var sky_mat := ProceduralSkyMaterial.new()
	sky_mat.sky_top_color = Color(0.22, 0.42, 0.75)
	sky_mat.sky_horizon_color = Color(0.68, 0.76, 0.85)
	sky_mat.ground_horizon_color = Color(0.68, 0.76, 0.85)
	sky_mat.ground_bottom_color = Color(0.3, 0.32, 0.3)
	var env := Environment.new()
	env.background_mode = Environment.BG_SKY
	env.sky = Sky.new()
	env.sky.sky_material = sky_mat
	env.ambient_light_source = Environment.AMBIENT_SOURCE_SKY
	env.ambient_light_energy = 0.6
	env.tonemap_mode = Environment.TONE_MAPPER_LINEAR
	env.fog_enabled = true
	env.fog_light_color = Color(0.68, 0.75, 0.84)
	env.fog_density = 0.00002
	env.fog_sky_affect = 0.0
	env.glow_enabled = true  # makes flashes and fireballs pop
	env.glow_hdr_threshold = 0.9
	var world := WorldEnvironment.new()
	world.environment = env
	add_child(world)


# Max hp per unit type from data/units.toml ([[unit]] blocks: id = "...", hp = ...).
func _unit_hps(path: String) -> Dictionary:
	var out := {}
	var f := FileAccess.open(path, FileAccess.READ)
	var id := ""
	while f != null and not f.eof_reached():
		var kv := f.get_line().split("#")[0].split("=", false, 1)
		if kv.size() != 2:
			continue
		match kv[0].strip_edges():
			"id":
				id = kv[1].strip_edges().trim_prefix("\"").trim_suffix("\"")
			"hp":
				out[id] = float(kv[1])
	return out


func max_hp(type: String) -> float:
	return maxf(max_hps.get(type, 100.0), 1.0)


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
	terrain.material.show_colormap = true
	var colors := _height_colors(img, float(r["min_height_m"]), float(r["max_height_m"]), float(meta.get("sea_level_m", -INF)))
	terrain.data.import_images([img, null, colors], Vector3.ZERO, 0.0, 1.0)


## Colour map by altitude (display only): plain green -> brown slopes -> grey rock -> snow.
## Everything at or below `sea` (map.toml sea_level_m) is drawn as water.
func _height_colors(heights: Image, lo: float, hi: float, sea: float) -> Image:
	var stops := [[0.0, Color(0.36, 0.48, 0.25)], [0.25, Color(0.55, 0.55, 0.32)], [0.5, Color(0.55, 0.42, 0.28)],
		[0.75, Color(0.5, 0.47, 0.44)], [0.92, Color(0.85, 0.85, 0.85)], [1.0, Color(0.97, 0.97, 0.98)]]
	var lut := PackedByteArray()
	for k in 256:
		var t := k / 255.0
		var c: Color = stops[-1][1]
		for s in range(stops.size() - 1):
			if t <= stops[s + 1][0]:
				c = stops[s][1].lerp(stops[s + 1][1], (t - stops[s][0]) / (stops[s + 1][0] - stops[s][0]))
				break
		lut.append_array([c.r8, c.g8, c.b8, 255])
	var src := heights.duplicate()
	if src.get_format() != Image.FORMAT_RF:
		src.convert(Image.FORMAT_RF)
	var hs: PackedFloat32Array = src.get_data().to_float32_array()
	var out := PackedByteArray()
	out.resize(hs.size() * 4)
	var scale := 255.0 / maxf(hi - lo, 1.0)
	var water := Color(0.16, 0.35, 0.62)
	for k: int in hs.size():
		var i: int = clampi(int((hs[k] - lo) * scale), 0, 255) * 4
		var o: int = k * 4
		if hs[k] <= sea + 0.05:  # r16 quantisation is ~0.04 m
			out[o] = water.r8
			out[o + 1] = water.g8
			out[o + 2] = water.b8
			out[o + 3] = 255
			continue
		out[o] = lut[i]
		out[o + 1] = lut[i + 1]
		out[o + 2] = lut[i + 2]
		out[o + 3] = 255
	return Image.create_from_data(src.get_width(), src.get_height(), false, Image.FORMAT_RGBA8, out)


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
	ui.set_connected(true)
	ui.banner("Bağlandı (%s).\nRakip bekleniyor..." % ui.name_box.text)
	_status("Bağlandı, rakip bekleniyor...")


## Saves a screenshot after the terrain has rendered, then quits (for checking visuals).
func _screenshot(path: String) -> void:
	await get_tree().create_timer(4.0).timeout
	get_viewport().get_texture().get_image().save_png(path)
	get_tree().quit()


func _on_message(m: Dictionary) -> void:
	match m.get("type"):
		"welcome":
			player_id = int(m.player_id)
			ui.banner("Oyuncu %d (%s).\nRakip bekleniyor..." % [player_id + 1, "Mavi, kuzey" if player_id == 0 else "Kırmızı, güney"])
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
				_status("Rota yok (birim %d): %s" % [int(m.unit), m.reason])
		"orders_accepted":
			kbg_spent = int(m.kbg_spent)
			kbg_budget = int(m.kbg_budget)
		"order_rejected":
			_status("Emir reddedildi: %s" % m.reason)
		"game_over":
			game_over = m
			ui.ready_button.disabled = true
			if not playing:
				_show_game_over()
		"error":
			_status("Sunucu hatası (%s): %s" % [m.code, m.message])


func _apply_turn_start(m: Dictionary) -> void:
	ui.banner("")
	turn = int(m.turn)
	deadline = float(m.deadline_secs)
	kbg_budget = int(m.view.kbg_budget)
	scores = m.view.scores
	zones = m.view.zones
	overlays.set_zones(zones, player_id, TEAM_COLORS)
	units.clear()
	orders.clear()
	previews.clear()
	kbg_spent = 0
	for u in m.view.own_units:
		var id := int(u.id)
		units[id] = {"id": id, "owner": player_id, "type": u.type_id, "pos": Vector2(u.pos.x, u.pos.y),
			"heading": u.heading_rad, "state": u.state, "quality": 3, "kind": "exact", "hp": u.hp,
			"fuel": u.fuel, "ammo": int(u.ammo), "emitting": u.emitting,
			"supply_fuel": u.supply_fuel, "supply_ammo": int(u.supply_ammo),
			"stance": u.get("stance", "free"), "connected": u.get("connected", true)}
	for c in m.view.contacts:
		var id := int(c.unit)
		units[id] = {"id": id, "owner": 1 - player_id, "type": c.type_id if c.type_id != null else "",
			"heading": 0.0, "state": "healthy", "quality": int(c.quality)}
		units[id].merge(_position(c.position), true)
	selection = selection.filter(_is_own_alive)
	targeting.clear()
	if game_over == null:
		ui.ready_button.disabled = false
		_status("Tur %d: planlama. Sol tık/sürükle seç, sağ tık rota noktası." % turn)
	else:
		_show_game_over()


# Position message -> {pos, kind, radius, angle}. A bearing has no position: pos is the ESM origin.
func _position(pos: Dictionary) -> Dictionary:
	match pos.kind:
		"exact":
			return {"pos": Vector2(pos.pos.x, pos.pos.y), "kind": "exact"}
		"bearing":
			return {"pos": Vector2(pos.origin.x, pos.origin.y), "kind": "bearing", "angle": float(pos.angle_rad)}
	return {"pos": Vector2(pos.center.x, pos.center.y), "kind": "area", "radius": float(pos.radius_m)}


func _submit_orders() -> void:
	if turn > 0 and not playing and game_over == null:
		net.send({"type": "submit_orders", "turn": turn, "orders": orders.to_wire()})


# After any order change: re-preview every group move and re-send the full list (the server replaces it).
func _orders_changed() -> void:
	previews.clear()
	for o in orders.moves():
		net.send({"type": "request_preview", "units": o.units,
			"waypoints": o.waypoints.map(func(p): return {"x": p.x, "y": p.y})})
	_submit_orders()


func _show_game_over() -> void:
	ui.show_game_over(game_over.winner, game_over.scores, player_id)
	scores = game_over.scores
	_status("Oyun bitti.")


# ---------------------------------------------------------------- playback

func _start_playback(evs: Array) -> void:
	events = evs
	event_index = 0
	play_time = 0.0
	playing = true
	keyframes.clear()
	logged.clear()
	for id in units:
		keyframes[id] = [[0.0, units[id].pos]]
	for e in events:
		if e.type == "unit_moved" or e.type == "track_gained":
			var p := _position(e.position)
			if p.kind == "bearing":
				continue
			var id := int(e.unit)
			if not keyframes.has(id):
				keyframes[id] = []
			keyframes[id].append([float(e.tick), p.pos])
	previews.clear()
	targeting.clear()
	ui.ready_button.disabled = true
	_status("Tur %d: çözüm oynatılıyor." % turn)


func _advance_playback(delta: float) -> void:
	play_time += delta
	var tick := play_time * TICKS_PER_SECOND
	while event_index < events.size() and float(events[event_index].tick) <= tick:
		_apply_event(events[event_index])
		event_index += 1
	for id in units:
		if units[id].kind != "bearing":
			units[id].pos = _interpolate(keyframes.get(id, []), tick, units[id].pos)
	if tick >= RESOLUTION_TICKS:
		playing = false
		if pending_turn_start != null:
			_apply_turn_start(pending_turn_start)
			pending_turn_start = null
		elif game_over != null:
			_show_game_over()


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
				units[id].kind = _position(e.position).kind
				if e.heading_rad != null:
					units[id].heading = e.heading_rad
		"track_gained":
			var id := int(e.unit)
			if not units.has(id):
				units[id] = {"id": id, "owner": 1 - player_id, "type": "", "heading": 0.0, "state": "healthy"}
			units[id].quality = int(e.quality)
			units[id].merge(_position(e.position), true)
		"track_lost":
			var id := int(e.unit)
			if units.has(id):
				units[id].quality = 0
				units[id].merge(_position(e.last_known), true)
				keyframes.erase(id)
		"unit_damaged":
			var id := int(e.unit)
			if units.has(id):
				var was: String = units[id].state
				units[id].state = e.state
				if e.get("hp") != null:
					units[id].hp = e.hp
				if was != e.state and e.state == "damaged":
					_log("%s hasar aldı" % _words(id), Color(1.0, 0.8, 0.4))
		"unit_destroyed":
			var id := int(e.unit)
			if units.has(id):
				units[id].state = "destroyed"
				if units[id].kind == "exact":
					fx.explosion(_unit_point(id, 5), 45.0 * views.zoom())
				_log("%s imha edildi!" % _words(id), Color(1.0, 0.35, 0.3) if units[id].owner == player_id else Color(0.5, 1.0, 0.5))
		"shot_fired":
			var s = units.get(int(e.shooter)) if e.shooter != null else null
			var t = units.get(int(e.target)) if e.target != null else null
			var k := views.zoom()
			if s != null and s.kind == "exact":
				fx.muzzle(_unit_point(int(e.shooter), 10), k)
			if t != null and t.kind == "exact":
				var to := _unit_point(int(e.target), 5)
				if s != null and s.kind == "exact":
					fx.tracer(_unit_point(int(e.shooter), 10), to, k)
				else:
					fx.tracer(to + Vector3(0, 60, 0) * k, to, k)  # unseen shooter: only the impact reads
			var key := "%s>%s" % [e.shooter, e.target]
			if not logged.has(key):
				logged[key] = true
				if s != null and s.owner == player_id:
					_log("%s %s ateş etti" % [_words(int(e.shooter)), _words(int(e.target), true) if t != null else "görünmeyen hedefe"])
				elif t != null and t.owner == player_id:
					var who := _words(int(e.shooter)) if s != null else "Görünmeyen bir düşman"
					_log("%s %s ateş etti" % [who, _words(int(e.target), true)], Color(1.0, 0.7, 0.6))
		"shell_fired":
			if e.shooter != null and units.has(int(e.shooter)):
				fx.muzzle(_unit_point(int(e.shooter), 20), views.zoom(), true)
				if not logged.has("shell%s" % e.shooter):
					logged["shell%s" % e.shooter] = true
					_log("%s atış yaptı" % _words(int(e.shooter)))
		"shell_impact":
			fx.explosion(to_world(Vector2(e.pos.x, e.pos.y)), float(e.radius_m))
			_log("Topçu mermisi düştü", Color(1.0, 0.75, 0.5))
		"zone_captured":
			for z in zones:
				if int(z.id) == int(e.zone):
					z.owner = e.player
					var who := "Biz" if int(e.player) == player_id else "Rakip"
					_status("%s bölgeyi ele geçirdi: %s" % [who, z.name_tr])
					if int(e.player) >= 0 and int(e.player) < 2:
						_log("%s %s geçti" % [z.name_tr, SIDE_DATIVE[int(e.player)]], TEAM_COLORS[int(e.player)].lightened(0.3))
			overlays.set_zones(zones, player_id, TEAM_COLORS)


func _words(id: int, dative := false) -> String:
	var u: Dictionary = units[id]
	return Hud.unit_words(u.type, id, u.owner == player_id, dative)


func _log(text: String, color := Color.WHITE) -> void:
	ui.log_event(text, color)


# World point of a unit (above its model) for effects.
func _unit_point(id: int, height: float) -> Vector3:
	var u: Dictionary = units[id]
	return to_world(u.pos, Models.lift(u.type) + height * views.zoom())


# ---------------------------------------------------------------- drawing

# Contacts without an exact, typed position: faded boxes (area contacts: a slab over the area).
# Exact contacts and own units are drawn as models by unit_views.gd.
func _draw_units() -> void:
	var f: Array = []
	for id in units:
		var u: Dictionary = units[id]
		if u.kind == "bearing" or u.owner == player_id or (u.quality >= 2 and u.kind == "exact"):
			continue  # bearings: a ray in _draw_lines
		var color: Color = DEAD_COLOR if u.state == "destroyed" else (TEAM_COLORS[u.owner] if u.owner >= 0 and u.owner < 2 else Color.WHITE)
		if u.kind == "area":
			var d: float = u.get("radius", 250.0) * 2.0
			f.append([Transform3D(Basis.from_scale(Vector3(d, 10, d)), to_world(u.pos, 5)), Color(color, 0.25 if u.quality > 0 else 0.12)])
		else:
			f.append([Transform3D(Basis.from_scale(APPROX_SIZE), to_world(u.pos, APPROX_SIZE.y * 0.5)), Color(color, 0.3 if u.quality > 0 else 0.15)])
	var mm := faded.multimesh
	if mm.instance_count != f.size():
		mm.instance_count = f.size()
	for i in f.size():
		mm.set_instance_transform(i, f[i][0])
		mm.set_instance_color(i, f[i][1])


func _draw_lines() -> void:
	var segs: Array = []
	var pulse := 0.5 + 0.5 * sin(Time.get_ticks_msec() / 200.0)
	var enemy: Color = TEAM_COLORS[1 - player_id] if player_id >= 0 and player_id < 2 else Color.WHITE
	for id in units:
		var u: Dictionary = units[id]
		if u.owner == player_id and u.get("emitting", false) and u.state != "destroyed":
			_circle(segs, u.pos, 120.0 + 60.0 * pulse, Color(0.3, 1.0, 1.0, 0.4 + 0.5 * pulse))
		if u.owner == player_id and u.state != "destroyed":
			if u.type == "hq":  # HQ: a square frame around the model
				_circle(segs, u.pos, 110.0, Color.WHITE, 4, 1, PI / 4)
			if not u.get("connected", true):  # cut off from HQ: dashed grey ring
				_circle(segs, u.pos, 90.0, Color(0.6, 0.6, 0.6), 24, 2)
		if u.owner != player_id and u.kind == "bearing":
			# ESM bearing: the enemy is somewhere along this ray; its position is unknown.
			var dir := Vector2(cos(u.angle), sin(u.angle))
			_polyline(segs, [u.pos, u.pos + dir * BEARING_LENGTH], Color(enemy, 0.45 if u.quality > 0 else 0.15))
	var im: ImmediateMesh = lines.mesh
	im.clear_surfaces()
	if segs.is_empty():
		return
	im.surface_begin(Mesh.PRIMITIVE_LINES)
	for seg in segs:
		im.surface_set_color(seg[2])
		im.surface_add_vertex(seg[0])
		im.surface_add_vertex(seg[1])
	im.surface_end()


# Line through the points, split every 500 m and draped above the terrain.
func _polyline(segs: Array, pts: Array, color: Color) -> void:
	for i in range(pts.size() - 1):
		var n := maxi(1, int(pts[i].distance_to(pts[i + 1]) / 500.0))
		for k in n:
			segs.append([to_world(pts[i].lerp(pts[i + 1], float(k) / n), 30),
				to_world(pts[i].lerp(pts[i + 1], float(k + 1) / n), 30), color])


# Polygon of n sides around c; step 2 draws every other side (dashed).
func _circle(segs: Array, c: Vector2, r: float, color: Color, n := 24, step := 1, rot := 0.0) -> void:
	for k in range(0, n, step):
		var a := TAU * k / n + rot
		var b := TAU * (k + 1) / n + rot
		segs.append([to_world(c + Vector2(cos(a), sin(a)) * r, 20), to_world(c + Vector2(cos(b), sin(b)) * r, 20), color])


# ---------------------------------------------------------------- input: selection and orders

func _is_own_alive(id) -> bool:
	return units.has(id) and units[id].owner == player_id and units[id].state != "destroyed"


func _can_order() -> bool:
	return turn > 0 and not playing and game_over == null


func _unhandled_input(event: InputEvent) -> void:
	if event is InputEventMouseButton:
		_mouse_button(event)
	elif event is InputEventMouseMotion and drag_start != null:
		drag_end = event.position
		if drag_start.distance_to(drag_end) > DRAG_MIN_PX:
			ui.show_drag(drag_start, drag_end)
	elif event is InputEventKey and event.pressed and not event.echo:
		_key(event)


func _mouse_button(event: InputEventMouseButton) -> void:
	match event.button_index:
		MOUSE_BUTTON_WHEEL_UP:
			if event.pressed:
				cam_dist = max(300.0, cam_dist * 0.85)
		MOUSE_BUTTON_WHEEL_DOWN:
			if event.pressed:
				cam_dist = min(40000.0, cam_dist / 0.85)
		MOUSE_BUTTON_LEFT:
			if event.pressed and not targeting.is_empty():
				_fire_at(event.position)
			elif event.pressed:
				drag_start = event.position
				drag_end = event.position
			elif drag_start != null:
				ui.drag_box.visible = false
				if drag_start.distance_to(event.position) > DRAG_MIN_PX:
					_select_box(Rect2(drag_start, Vector2.ZERO).expand(event.position), event.shift_pressed)
				else:
					_select_click(event.position, event.shift_pressed)
				drag_start = null
		MOUSE_BUTTON_RIGHT:
			if event.pressed and _can_order() and not selection.is_empty():
				var p = _ground_point(event.position)
				var ids := _orderable(selection)
				if p != null and not ids.is_empty():
					orders.add_waypoint(ids, p)
					_orders_changed()


func _select_click(screen: Vector2, shift: bool) -> void:
	var id := _pick_own_unit(screen)
	if not shift:
		selection = [] if id < 0 else [id]
	elif id >= 0:
		if id in selection:
			selection.erase(id)
		else:
			selection.append(id)


func _select_box(rect: Rect2, shift: bool) -> void:
	if not shift:
		selection = []
	for id in units:
		if not _is_own_alive(id) or id in selection:
			continue
		var w := to_world(units[id].pos, 20)
		if not camera.is_position_behind(w) and rect.has_point(camera.unproject_position(w)):
			selection.append(id)


func _key(event: InputEventKey) -> void:
	var digit := event.keycode - KEY_0
	if digit >= 1 and digit <= 9:
		if event.ctrl_pressed:
			groups[digit] = selection.duplicate()
			_status("Grup %d kaydedildi (%d birim)." % [digit, selection.size()])
		else:
			selection = groups.get(digit, []).filter(_is_own_alive)
		return
	if event.keycode == KEY_ESCAPE and not targeting.is_empty():
		targeting.clear()
		_status("Atış görevi iptal.")
		return
	if not _can_order() or selection.is_empty():
		return
	if event.keycode in [KEY_DELETE, KEY_BACKSPACE]:
		orders.remove(selection)
		_orders_changed()
		return
	if not event.keycode in [KEY_S, KEY_F, KEY_T, KEY_R]:
		return
	var ids := _orderable(selection)
	if ids.is_empty():
		return
	match event.keycode:
		KEY_S:
			orders.stop(ids)
			_orders_changed()
		KEY_F:
			var cur = orders.pending_stance(ids[0])
			if cur == null:
				cur = units[ids[0]].stance
			var next: String = STANCES[(STANCES.find(cur) + 1) % STANCES.size()]
			orders.set_stance(ids, next)
			_orders_changed()
			_status("Ateş kuralı: %s (%d birim)" % [Hud.STANCE_NAMES[next], ids.size()])
		KEY_T:
			targeting = ids.filter(func(id): return units[id].type in ARTILLERY_TYPES)
			if targeting.is_empty():
				_status("Seçimde obüs veya füze rampası yok.")
			else:
				_status("Atış görevi: hedef için sol tık (arazi veya düşman), Esc iptal.")
		KEY_R:
			var emitting := {}
			for id in ids:
				if units[id].type in EMITTER_TYPES:
					emitting[id] = units[id].emitting
			if emitting.is_empty():
				_status("Seçimde radar veya karıştırıcı yok.")
				return
			orders.toggle_emitter(emitting)
			_orders_changed()


# Selected units that can take orders now; reports cut-off ones (K-083) in the status line.
func _orderable(ids: Array) -> Array:
	var out := ids.filter(func(id): return units[id].get("connected", true))
	if out.size() < ids.size():
		_status("Bağlantı yok: %d birim emir alamaz" % (ids.size() - out.size()))
	return out


# Targeting click: an enemy contact near the cursor gives a unit mission, else the ground point.
func _fire_at(screen: Vector2) -> void:
	var enemy := _pick_unit(screen, _is_enemy_target)
	if enemy >= 0:
		add_mission({"kind": "unit", "unit": enemy})
		return
	var p = _ground_point(screen)
	if p == null:
		_status("Hedef arazide değil.")
		return
	add_mission({"kind": "point", "pos": p})


func add_mission(target: Dictionary) -> void:
	if targeting.is_empty() or not _can_order():
		return
	orders.fire_mission(targeting, target)
	_status("Atış görevi: %d birim -> %s" % [targeting.size(),
		"nokta" if target.kind == "point" else "birim #%d" % target.unit])
	targeting.clear()
	_orders_changed()


func _is_enemy_target(id) -> bool:
	var u: Dictionary = units[id]
	return u.owner != player_id and u.state != "destroyed" and u.kind != "bearing" and u.quality > 0


func _pick_own_unit(screen: Vector2) -> int:
	return _pick_unit(screen, _is_own_alive)


# Unit (passing the filter) whose box is nearest the cursor within 30 px, or -1.
func _pick_unit(screen: Vector2, filter: Callable) -> int:
	var best := -1
	var best_d := 30.0
	for id in units:
		if not filter.call(id):
			continue
		var w := to_world(units[id].pos, 20)
		if camera.is_position_behind(w):
			continue
		var d := camera.unproject_position(w).distance_to(screen)
		if d < best_d:
			best_d = d
			best = id
	return best


# Terrain point under the cursor as sim metres, or null.
func _ground_point(screen: Vector2):
	var from := camera.project_ray_origin(screen)
	var dir := camera.project_ray_normal(screen)
	var hit: Vector3
	if terrain != null:
		hit = terrain.get_intersection(from, dir)
	else:
		hit = from + dir * (-from.y / dir.y) if dir.y < 0.0 else Vector3(INF, 0, 0)
	if not is_finite(hit.x) or hit.z > 3.0e38:
		return null
	return Vector2(hit.x, hit.z)


func _move_camera(delta: float) -> void:
	var pan := Vector3.ZERO
	if Input.is_key_pressed(KEY_W) or Input.is_key_pressed(KEY_UP): pan.z -= 1
	# While units are selected S is the stop key, not a pan key.
	if (Input.is_key_pressed(KEY_S) and selection.is_empty()) or Input.is_key_pressed(KEY_DOWN): pan.z += 1
	if Input.is_key_pressed(KEY_A) or Input.is_key_pressed(KEY_LEFT): pan.x -= 1
	if Input.is_key_pressed(KEY_D) or Input.is_key_pressed(KEY_RIGHT): pan.x += 1
	if Input.is_key_pressed(KEY_Q): cam_yaw += delta
	if Input.is_key_pressed(KEY_E): cam_yaw -= delta
	cam_target += pan.rotated(Vector3.UP, cam_yaw) * cam_dist * delta
	cam_target.y = ground(Vector2(cam_target.x, cam_target.z))
	var offset := Vector3(0, cam_dist * 0.8, cam_dist * 0.6).rotated(Vector3.UP, cam_yaw)
	camera.look_at_from_position(cam_target + offset, cam_target)


# ---------------------------------------------------------------- HUD

func _update_hud() -> void:
	var score := ui.score_text(scores, player_id)
	if playing:
		ui.line.text = "  Tur %d  çözüm %.1f / 20 sn  %s" % [turn, play_time, score]
	elif turn > 0:
		ui.line.text = "  Tur %d  kalan %d sn  KBG: %d / %d  %s" % [turn, ceil(deadline), kbg_spent, kbg_budget, score]
	ui.fps.text = "FPS %d" % Engine.get_frames_per_second()
	var sel := selection.filter(func(id): return units.has(id)).map(func(id): return units[id])
	ui.show_selection(sel, orders)


func _status(text: String) -> void:
	ui.status.text = text


