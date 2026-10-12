# SPDX-License-Identifier: AGPL-3.0-or-later
# Short combat effects (display only): muzzle flashes, tracers, impact puffs, explosions, smoke.
# Mesh nodes and particle emitters are pooled and reused; nothing is allocated per frame.
extends Node3D

const FX_SHADER := """
shader_type spatial;
render_mode unshaded, %s, depth_draw_never, cull_back;
instance uniform vec4 tint : source_color = vec4(1.0);
void fragment() {
	ALBEDO = tint.rgb * %s;
	ALPHA = tint.a * pow(clamp(dot(NORMAL, VIEW), 0.0, 1.0), %s);
}
"""
const FIRE := [Color(1.0, 0.95, 0.7), Color(1.0, 0.55, 0.1), Color(0.5, 0.12, 0.05)]

var add_mat := _shader_mat("blend_add", "2.5", "0.8")  # bright enough to bloom
var mix_mat := _shader_mat("blend_mix", "1.0", "1.2")
var meshes := {}  # kind -> Mesh
var pools := {}  # kind -> Array of idle MeshInstance3D
var active: Array = []  # [node, kind, t, life, a: Vector3, b: Vector3, size: float, color: Color]
var bursts: Array = []  # one-shot GPUParticles3D, reused round robin
var burst_next := 0
var smoke_quad: QuadMesh


func _ready() -> void:
	var sphere := SphereMesh.new()
	sphere.radius = 1.0
	sphere.height = 2.0
	sphere.radial_segments = 12
	sphere.rings = 6
	var torus := TorusMesh.new()
	torus.inner_radius = 0.7
	torus.outer_radius = 1.0
	torus.rings = 16
	torus.ring_segments = 6
	var box := BoxMesh.new()  # tracer: unit box stretched along its path
	meshes = {"glow": sphere, "fire": sphere, "smoke": sphere, "ring": torus, "tracer": box}
	smoke_quad = QuadMesh.new()
	var m := StandardMaterial3D.new()
	m.billboard_mode = BaseMaterial3D.BILLBOARD_PARTICLES
	m.billboard_keep_scale = true
	m.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
	m.vertex_color_use_as_albedo = true
	m.vertex_color_is_srgb = true
	m.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	var tex := GradientTexture2D.new()
	tex.fill = GradientTexture2D.FILL_RADIAL
	tex.fill_from = Vector2(0.5, 0.5)
	tex.fill_to = Vector2(1.0, 0.5)
	tex.gradient = Gradient.new()
	tex.gradient.set_color(0, Color.WHITE)
	tex.gradient.set_color(1, Color(1, 1, 1, 0))
	m.albedo_texture = tex
	smoke_quad.material = m
	for i in 12:
		var p := _emitter(true, 32, 2.5)
		bursts.append(p)
		add_child(p)


func _process(delta: float) -> void:
	var i := 0
	while i < active.size():
		var e: Array = active[i]
		e[2] += delta
		if e[2] >= e[3]:
			e[0].visible = false
			pools[e[1]].append(e[0])
			active[i] = active[-1]
			active.pop_back()
			continue
		_animate(e)
		i += 1


# ---------------------------------------------------------------- public effects (k = zoom size factor)

func muzzle(p: Vector3, k: float, big := false) -> void:
	_spawn("glow", p, p, (14.0 if big else 6.0) * k, FIRE[0], 0.25 if big else 0.15)
	if big:
		_spawn("ring", p, p, 12.0 * k, Color(0.8, 0.78, 0.72, 0.6), 1.6)
		_spawn("smoke", p, p + Vector3(0, 10, 0) * k, 8.0 * k, Color(0.75, 0.73, 0.7, 0.6), 2.0, 0.05)


## Tracer from a to b, then a small puff at b.
func tracer(a: Vector3, b: Vector3, k: float) -> void:
	var life := clampf(a.distance_to(b) / 1500.0, 0.25, 0.6)  # slower than real so the eye can follow
	_spawn("tracer", a, b, 3.0 * k, Color(1.0, 0.8, 0.35), life)
	_spawn("glow", b, b, 5.0 * k, FIRE[1], 0.2, life)
	_spawn("smoke", b, b + Vector3(0, 6, 0) * k, 5.0 * k, Color(0.45, 0.42, 0.38, 0.6), 1.2, life)


## Explosion of radius r (metres) at p: fireball, dust ring, smoke puffs and debris particles.
func explosion(p: Vector3, r: float) -> void:
	_spawn("glow", p, p, r * 1.6, FIRE[0], 0.35)
	_spawn("fire", p + Vector3(0, r * 0.2, 0), p + Vector3(0, r * 0.7, 0), r * 0.75, FIRE[0], 1.1)
	for k in 3:  # lumps so the fireball is not a perfect ball
		var off := Vector3(cos(k * 2.1 + 0.5), 0.3, sin(k * 2.1 + 0.5)) * r * 0.45
		_spawn("fire", p + off, p + off * 1.6 + Vector3(0, r * 0.5, 0), r * 0.45, FIRE[0], 0.8, 0.05 * k)
	_spawn("ring", p, p, r * 1.3, Color(0.5, 0.43, 0.33, 0.7), 1.8)
	for k in 3:
		var off := Vector3(cos(k * 2.1), 0, sin(k * 2.1)) * r * 0.35
		_spawn("smoke", p + off, p + off + Vector3(0, r * 1.5, 0), r * 0.6, Color(0.13, 0.12, 0.11, 0.85), 3.5, 0.2 + 0.15 * k)
	var b: GPUParticles3D = bursts[burst_next]
	burst_next = (burst_next + 1) % bursts.size()
	b.position = p
	b.scale = Vector3.ONE * (r / 40.0)
	b.restart()


## Continuous smoke emitter for a damaged unit or a wreck (caller owns the node).
func smoke_emitter(dark: bool) -> GPUParticles3D:
	var e := _emitter(false, 16 if dark else 10, 5.0 if dark else 3.5)
	var pm: ParticleProcessMaterial = e.process_material
	pm.color_ramp.gradient.set_color(0, Color(0.1, 0.09, 0.08, 0.95) if dark else Color(0.3, 0.29, 0.28, 0.8))
	return e


# ---------------------------------------------------------------- internals

func _spawn(kind: String, a: Vector3, b: Vector3, size: float, color: Color, life: float, delay := 0.0) -> void:
	var pool: Array = pools.get_or_add(kind, [])
	var node: MeshInstance3D
	if pool.is_empty():
		node = MeshInstance3D.new()
		node.mesh = meshes[kind]
		node.material_override = add_mat if kind == "glow" or kind == "tracer" else mix_mat
		node.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
		add_child(node)
	else:
		node = pool.pop_back()
	node.visible = false
	active.append([node, kind, -delay, life, a, b, size, color])


func _animate(e: Array) -> void:
	var node: MeshInstance3D = e[0]
	if e[2] < 0.0:
		return
	node.visible = true
	var u: float = e[2] / e[3]
	var a: Vector3 = e[4]
	var b: Vector3 = e[5]
	var s: float = e[6]
	var c: Color = e[7]
	match e[1]:
		"tracer":
			var len := minf(a.distance_to(b) * 0.3, 60.0 * s)
			var dir := (b - a).normalized()
			var head := a.lerp(b, u)
			node.transform = Transform3D(Basis.looking_at(dir).scaled(Vector3(s, s, len)), head - dir * len * 0.5)
			node.set_instance_shader_parameter("tint", c)
		"glow":
			node.transform = Transform3D(Basis.from_scale(Vector3.ONE * s * (0.4 + 0.8 * sqrt(u))), a)
			var col: Color = FIRE[0].lerp(FIRE[1], minf(u * 2.0, 1.0)) if c == FIRE[0] else c
			if u > 0.5 and c == FIRE[0]:
				col = FIRE[1].lerp(FIRE[2], (u - 0.5) * 2.0)
			node.set_instance_shader_parameter("tint", Color(col, (1.0 - u) * (1.0 - u)))
		"fire":
			node.transform = Transform3D(Basis.from_scale(Vector3.ONE * s * (0.5 + 0.7 * sqrt(u))), a.lerp(b, u))
			var col: Color = FIRE[0].lerp(FIRE[1], minf(u * 3.0, 1.0)) if u < 0.34 else FIRE[1].lerp(FIRE[2], (u - 0.34) * 1.5)
			node.set_instance_shader_parameter("tint", Color(col, 1.0 - u * u))
		"smoke":
			node.transform = Transform3D(Basis.from_scale(Vector3.ONE * s * (0.5 + u)), a.lerp(b, sqrt(u)))
			node.set_instance_shader_parameter("tint", Color(c, c.a * (1.0 - u)))
		"ring":
			node.transform = Transform3D(Basis.from_scale(Vector3(1, 0.25, 1) * s * (0.2 + 1.3 * sqrt(u))), a)
			node.set_instance_shader_parameter("tint", Color(c, c.a * (1.0 - u)))


func _shader_mat(blend: String, boost: String, edge: String) -> ShaderMaterial:
	var sh := Shader.new()
	sh.code = FX_SHADER % [blend, boost, edge]
	var m := ShaderMaterial.new()
	m.shader = sh
	return m


# Smoke/debris particles: one-shot bursts (local, scaled with the blast) or a rising trail (world space).
func _emitter(one_shot: bool, amount: int, life: float) -> GPUParticles3D:
	var e := GPUParticles3D.new()
	e.amount = amount
	e.lifetime = life
	e.one_shot = one_shot
	e.emitting = not one_shot
	e.explosiveness = 0.9 if one_shot else 0.0
	e.local_coords = one_shot
	e.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	e.visibility_aabb = AABB(Vector3(-200, -50, -200), Vector3(400, 400, 400))
	e.draw_pass_1 = smoke_quad
	var pm := ParticleProcessMaterial.new()
	pm.direction = Vector3.UP
	pm.spread = 70.0 if one_shot else 12.0
	pm.initial_velocity_min = 25.0 if one_shot else 20.0
	pm.initial_velocity_max = 70.0 if one_shot else 30.0
	pm.gravity = Vector3(0, -25, 0) if one_shot else Vector3(4, 3, 0)
	pm.damping_min = 8.0 if one_shot else 3.0
	pm.damping_max = 15.0 if one_shot else 5.0
	pm.scale_min = 12.0 if one_shot else 22.0
	pm.scale_max = 24.0 if one_shot else 34.0
	var ramp := GradientTexture1D.new()
	ramp.gradient = Gradient.new()
	ramp.gradient.set_color(0, Color(0.25, 0.22, 0.2, 0.9) if one_shot else Color(0.35, 0.34, 0.33, 0.6))
	ramp.gradient.set_color(1, Color(0.4, 0.4, 0.4, 0.0))
	pm.color_ramp = ramp
	e.process_material = pm
	return e
