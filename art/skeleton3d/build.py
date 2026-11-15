# Builds the 3D skeleton avatar: an original skeleton bust modelled in Blender, rendered as a few
# frames per app state, then joined into one strip per state for ui/avatars/skeleton3d/.
# Run (Blender 4.2+, headless):
#   blender --background --factory-startup --python art/skeleton3d/build.py -- ui/avatars/skeleton3d TEMPDIR
# Writes <state>.webp strips and strips.js (frame counts and timing) into the first folder; the
# loose frames and the .blend file go to TEMPDIR. Blender's console is hidden in the Store build,
# so progress goes to TEMPDIR/log.txt and TEMPDIR/done.txt says "ok" or "error".
import bpy, math, os, sys, traceback
from mathutils import Vector

ARGS = sys.argv[sys.argv.index("--") + 1:]
DEST, OUT = os.path.abspath(ARGS[0]), os.path.abspath(ARGS[1])
os.makedirs(DEST, exist_ok=True); os.makedirs(OUT, exist_ok=True)
log = open(os.path.join(OUT, "log.txt"), "w")
def L(*a):
    log.write(" ".join(map(str, a)) + "\n"); log.flush()

def main():
    bpy.ops.wm.read_factory_settings(use_empty=True)
    scene = bpy.context.scene
    coll = scene.collection

    def active():
        return bpy.context.view_layer.objects.active

    # ---------- materials ----------
    def principled(name, color, rough=0.5):
        m = bpy.data.materials.new(name); m.use_nodes = True
        b = m.node_tree.nodes["Principled BSDF"]
        b.inputs["Base Color"].default_value = (*color, 1)
        b.inputs["Roughness"].default_value = rough
        return m, b

    def bone_material():
        m, b = principled("Bone", (0.70, 0.62, 0.45), 0.6)
        nt = m.node_tree; n = nt.nodes
        tex = n.new("ShaderNodeTexNoise"); tex.inputs["Scale"].default_value = 22; tex.inputs["Detail"].default_value = 8
        ramp = n.new("ShaderNodeValToRGB")
        ramp.color_ramp.elements[0].color = (0.50, 0.41, 0.27, 1)
        ramp.color_ramp.elements[1].color = (0.80, 0.72, 0.55, 1)
        ramp.color_ramp.elements[0].position = 0.3
        ramp.color_ramp.elements[1].position = 0.7
        bump = n.new("ShaderNodeBump"); bump.inputs["Strength"].default_value = 0.12
        nt.links.new(tex.outputs["Fac"], ramp.inputs["Fac"])
        nt.links.new(ramp.outputs["Color"], b.inputs["Base Color"])
        nt.links.new(tex.outputs["Fac"], bump.inputs["Height"])
        nt.links.new(bump.outputs["Normal"], b.inputs["Normal"])
        b.inputs["Subsurface Weight"].default_value = 0.08
        return m

    BONE = bone_material()
    VOID, _ = principled("Void", (0.012, 0.008, 0.008), 0.9)
    EMBER, eb = principled("Ember", (0.6, 0.02, 0.02), 0.4)
    eb.inputs["Emission Color"].default_value = (1.0, 0.08, 0.04, 1)
    eb.inputs["Emission Strength"].default_value = 5
    SLATE, _ = principled("Slate", (0.13, 0.17, 0.23), 0.5)
    KEYCAP, _ = principled("Keycap", (0.72, 0.77, 0.83), 0.4)

    # ---------- helpers ----------
    def finish(o, m, parent=None, smooth=True):
        if m: o.data.materials.append(m)
        if smooth and o.type == "MESH":
            for p in o.data.polygons: p.use_smooth = True
        if parent:
            bpy.context.view_layer.update()
            o.parent = parent
            o.matrix_parent_inverse = parent.matrix_world.inverted()
        return o

    def empty(name, loc, parent=None):
        o = bpy.data.objects.new(name, None); o.location = loc; coll.objects.link(o)
        return finish(o, None, parent, False)

    def sphere(name, loc, scale, m, parent=None, seg=48):
        bpy.ops.mesh.primitive_uv_sphere_add(segments=seg, ring_count=seg // 2, location=loc)
        o = active(); o.name = name; o.scale = scale
        bpy.ops.object.transform_apply(scale=True)
        return finish(o, m, parent)

    def rbox(name, loc, half, m, parent=None, bevel=0.05):
        bpy.ops.mesh.primitive_cube_add(location=loc)
        o = active(); o.name = name; o.scale = half
        bpy.ops.object.transform_apply(scale=True)
        mod = o.modifiers.new("bevel", "BEVEL"); mod.width = bevel; mod.segments = 5
        bpy.ops.object.modifier_apply(modifier=mod.name)
        return finish(o, m, parent)

    def limb(name, a, b, r, m, parent=None, knobs=1.35):
        a, b = Vector(a), Vector(b); d = b - a
        bpy.ops.mesh.primitive_cylinder_add(radius=r, depth=d.length, location=(a + b) / 2, vertices=24)
        o = active(); o.name = name
        o.rotation_mode = "QUATERNION"; o.rotation_quaternion = Vector((0, 0, 1)).rotation_difference(d)
        finish(o, m, parent)
        if knobs:
            for i, p in enumerate((a, b)):
                sphere(f"{name}_end{i}", p, (r * knobs,) * 3, m, parent, seg=24)
        return o

    def cut(target, cutter):
        mod = target.modifiers.new("cut", "BOOLEAN")
        mod.operation = "DIFFERENCE"; mod.object = cutter; mod.solver = "EXACT"; mod.material_mode = "TRANSFER"
        bpy.context.view_layer.objects.active = target
        bpy.ops.object.modifier_apply(modifier=mod.name)

    def ring(name, center, rx, ry, tilt, thick, m):
        cu = bpy.data.curves.new(name, "CURVE"); cu.dimensions = "3D"
        cu.bevel_depth = thick; cu.bevel_resolution = 4
        sp = cu.splines.new("NURBS"); n = 28; sp.points.add(n - 1)
        for i in range(n):
            t = 2 * math.pi * i / n
            sp.points[i].co = (rx * math.cos(t), ry * math.sin(t), 0, 1)
        sp.use_cyclic_u = True; sp.order_u = 4
        o = bpy.data.objects.new(name, cu); o.location = center; o.rotation_euler = (math.radians(tilt), 0, 0)
        coll.objects.link(o); cu.materials.append(m)
        return o

    # ---------- body ----------
    for i, z in enumerate([0.26 + 0.088 * k for k in range(9)]):
        limb(f"vert{i}", (0, 0.1, z), (0, 0.1, z + 0.055), 0.05, BONE, knobs=0)
    for i, (z, w) in enumerate([(0.79, 0.37), (0.67, 0.40), (0.55, 0.39), (0.43, 0.35), (0.32, 0.29)]):
        ring(f"rib{i}", (0, 0.03, z), w, 0.30, 16, 0.027, BONE)
    rbox("sternum", (0, -0.27, 0.6), (0.045, 0.03, 0.2), BONE, bevel=0.025)
    for s in (-1, 1):
        limb(f"clavicle{s}", (0.06 * s, -0.26, 0.86), (0.53 * s, -0.02, 0.82), 0.032, BONE)

    def arm(side):
        sx = 0.57 * side
        sh = empty(f"shoulder{side}", (sx, 0, 0.8))
        sphere(f"ball{side}", (sx, 0, 0.8), (0.078,) * 3, BONE, sh)
        limb(f"humerus{side}", (sx, 0, 0.8), (sx, 0, 0.37), 0.04, BONE, sh)
        el = empty(f"elbow{side}", (sx, 0, 0.37), sh)
        sphere(f"elbowball{side}", (sx, 0, 0.37), (0.056,) * 3, BONE, el)
        for dx in (-0.024, 0.024):
            limb(f"forearm{side}{dx}", (sx + dx, 0, 0.36), (sx + dx, 0, 0.0), 0.021, BONE, el)
        rbox(f"hand{side}", (sx, 0, -0.06), (0.06, 0.03, 0.05), BONE, el, bevel=0.025)
        for k, dx in enumerate((-0.04, 0, 0.04)):
            limb(f"finger{side}{k}", (sx + dx, 0, -0.11), (sx + dx, 0, -0.21), 0.013, BONE, el)
        return sh, el

    sh_l, el_l = arm(-1)
    sh_r, el_r = arm(1)

    # ---------- head ----------
    head = empty("head", (0, 0, 0.98))
    cranium = sphere("cranium", (0, 0.03, 1.56), (0.45, 0.48, 0.43), BONE, seg=64)
    face = rbox("face", (0, -0.2, 1.29), (0.29, 0.23, 0.17), BONE, bevel=0.12)
    for s in (-1, 1):
        sphere(f"cheek{s}", (0.27 * s, -0.25, 1.33), (0.12, 0.1, 0.075), BONE, head)
    jaw = rbox("jaw", (0, -0.18, 1.06), (0.25, 0.21, 0.075), BONE, bevel=0.07)
    for s in (-1, 1):
        cutter = sphere(f"socketcut{s}", (0.175 * s, -0.43, 1.46), (0.145, 0.15, 0.125), VOID)
        cut(cranium, cutter); cut(face, cutter)
        bpy.data.objects.remove(cutter)
        sphere(f"ember{s}", (0.175 * s, -0.37, 1.45), (0.034,) * 3, EMBER, head, seg=24)
    nose = sphere("nosecut", (0, -0.43, 1.27), (0.05, 0.14, 0.085), VOID)
    cut(face, nose); bpy.data.objects.remove(nose)
    for o in (cranium, face, jaw):
        finish(o, None, head)
    for row, z in ((0, 1.145), (1, 1.105)):
        for k in range(7):
            x = -0.15 + 0.05 * k
            rbox(f"tooth{row}{k}", (x, -0.405 + 0.6 * x * x, z), (0.02, 0.02, 0.026), BONE, head, bevel=0.012)
    for i, z in enumerate((0.9, 0.97)):
        limb(f"neck{i}", (0, 0.05, z), (0, 0.05, z + 0.05), 0.055, BONE, knobs=0)

    # ---------- keyboard (typing only) ----------
    kb = empty("keyboard", (0, -0.66, 0.274))
    rbox("kbbase", (0, -0.66, 0.274), (0.78, 0.17, 0.03), SLATE, kb, bevel=0.02)
    for r, y in enumerate((-0.73, -0.61)):
        for k in range(11):
            rbox(f"key{r}{k}", (-0.70 + 0.14 * k, y, 0.312), (0.055, 0.045, 0.012), KEYCAP, kb, bevel=0.01)

    # ---------- camera and light ----------
    target = empty("target", (0, 0, 0.98))
    def aim(o):
        c = o.constraints.new("TRACK_TO"); c.target = target; c.track_axis = "TRACK_NEGATIVE_Z"; c.up_axis = "UP_Y"
    cam_data = bpy.data.cameras.new("cam"); cam_data.type = "ORTHO"; cam_data.ortho_scale = 2.6
    cam = bpy.data.objects.new("cam", cam_data); cam.location = (-1.5, -5.6, 2.4); coll.objects.link(cam); aim(cam)
    scene.camera = cam
    for name, loc, energy, size, color in (
        ("key", (-3, -4, 4.5), 650, 2.5, (1.0, 0.96, 0.9)),
        ("fill", (3.5, -3, 1.2), 180, 4, (0.8, 0.85, 1.0)),
        ("rim", (2.5, 3.5, 3.5), 700, 2, (1.0, 0.75, 0.6)),
    ):
        ld = bpy.data.lights.new(name, "AREA"); ld.energy = energy; ld.size = size; ld.color = color
        lo = bpy.data.objects.new(name, ld); lo.location = loc; coll.objects.link(lo); aim(lo)
    world = bpy.data.worlds.new("world"); scene.world = world; world.use_nodes = True
    world.node_tree.nodes["Background"].inputs["Color"].default_value = (0.05, 0.05, 0.06, 1)

    # ---------- render settings ----------
    scene.render.engine = "CYCLES"
    scene.cycles.device = "CPU"; scene.cycles.samples = 64; scene.cycles.use_denoising = True
    scene.render.resolution_x = scene.render.resolution_y = 280
    scene.render.film_transparent = True
    scene.render.image_settings.file_format = "PNG"; scene.render.image_settings.color_mode = "RGBA"
    scene.view_settings.view_transform = "AgX"; scene.view_settings.look = "AgX - Medium High Contrast"
    scene.use_nodes = True
    tree = scene.node_tree
    for n in list(tree.nodes): tree.nodes.remove(n)
    rl = tree.nodes.new("CompositorNodeRLayers")
    glare = tree.nodes.new("CompositorNodeGlare"); glare.glare_type = "FOG_GLOW"; glare.threshold = 2.0; glare.size = 5
    comp = tree.nodes.new("CompositorNodeComposite")
    tree.links.new(rl.outputs["Image"], glare.inputs["Image"])
    tree.links.new(glare.outputs["Image"], comp.inputs["Image"])

    # ---------- props: an open book (reading) and a hand bell (done) ----------
    PAGE, _ = principled("Page", (0.93, 0.91, 0.85), 0.8)
    COVER, _ = principled("Cover", (0.20, 0.06, 0.025), 0.5)
    INK, _ = principled("Ink", (0.35, 0.37, 0.42), 0.8)
    BRASS, brb = principled("Brass", (0.85, 0.62, 0.18), 0.3)
    brb.inputs["Metallic"].default_value = 1.0
    WOOD, _ = principled("Wood", (0.35, 0.18, 0.07), 0.6)
    # The book is held the way a reader holds it: pages toward the skull, spine toward the viewer,
    # each half folded back from the spine toward a hand, and the top tipped back toward the face.
    book = empty("book", (0, -0.6, 0.66))
    rbox("spine", (0, -0.635, 0.66), (0.03, 0.02, 0.35), COVER, book, bevel=0.012)
    for s in (-1, 1):
        half = empty(f"half{s}", (0, -0.62, 0.66), book)
        rbox(f"cover{s}", (0.26 * s, -0.62, 0.66), (0.26, 0.012, 0.35), COVER, half, bevel=0.01)
        rbox(f"page{s}", (0.25 * s, -0.596, 0.66), (0.24, 0.012, 0.33), PAGE, half, bevel=0.008)
        for k in range(6):
            rbox(f"line{s}{k}", (0.25 * s, -0.583, 0.86 - 0.075 * k), (0.17 if k % 3 else 0.12, 0.002, 0.008), INK, half, bevel=0.002)
        half.rotation_euler = (0, 0, math.radians(25 * s))
    book.rotation_euler = (math.radians(-30), 0, 0)
    # The bell hangs from the right hand. Its pivot turns against the arm so it always hangs down.
    bell = empty("bell", (0.57, 0, -0.17), el_r)
    limb("bellhandle", (0.57, 0, -0.17), (0.57, 0, -0.30), 0.026, WOOD, bell, knobs=0)
    bpy.ops.mesh.primitive_cone_add(vertices=48, radius1=0.19, radius2=0.06, depth=0.24, location=(0.57, 0, -0.42))
    finish(active(), BRASS, bell)
    sphere("clapper", (0.57, 0, -0.55), (0.04,) * 3, BRASS, bell, seg=24)

    # ---------- the states (degrees) ----------
    IDLE = dict(head=(0, 0, 0), sl=(0, 8, 0), sr=(0, -8, 0), el=(0, 0, 0), er=(0, 0, 0), glow=5, props=())
    def f(base, **kw):
        d = dict(base); d.update(kw); return d
    TYPE = f(IDLE, head=(12, 0, 0), sl=(-22, 0, 0), sr=(-22, 0, 0), el=(-62, 0, 0), er=(-62, 0, 0), props=("keyboard",))
    WAVE = f(IDLE, head=(0, 8, -6), sr=(0, -115, 0), glow=8)
    READ = f(IDLE, sl=(-15, 0, 0), sr=(-15, 0, 0), el=(-113, 0, 0), er=(-113, 0, 0), props=("book",))
    RING = f(IDLE, head=(0, -6, 6), sr=(0, -60, 0), er=(0, -130, 0), props=("bell",))
    SCARED = f(IDLE, sl=(0, 35, 0), sr=(0, -35, 0), glow=12)
    def bounce(seq):
        return seq + seq[-2:0:-1]
    # Each state: its frames in play order, the seconds one pass takes, and whether it repeats.
    # A state that plays once holds its last frame.
    states = {
        "asleep": ([f(IDLE, head=(18, 0, 10), sl=(0, b, 0), sr=(0, -b, 0), glow=0.3) for b in (7, 9)], 3.0, True),
        "idle": (bounce([f(IDLE, head=(a, 0, 0), sl=(0, b, 0), sr=(0, -b, 0)) for a, b in ((0, 8), (2, 9), (3, 10), (2, 9))]), 2.4, True),
        "typing": ([f(TYPE, head=(12, 0, h), el=(l, 0, 0), er=(r, 0, 0))
                    for l, r, h in ((-60, -66, 2), (-62, -62, 0), (-66, -60, -2), (-62, -62, 0))], 0.6, True),
        "reading": (bounce([f(READ, head=(10, 0, z)) for z in (-10, -4, 2, 8)]), 2.6, True),
        "thinking": (bounce([f(IDLE, head=(-8, 0, z)) for z in (-18, -11, -4, 4, 11, 18)]), 3.0, True),
        "done": ([f(RING, bell=a) for a in (0, -22, 18, -12, 6, 0)], 1.6, False),
        "needs-you": (bounce([f(WAVE, er=(0, a, 0)) for a in (-35, -50, -65, -80, -95)]), 1.4, True),
        "error": ([f(SCARED, head=(0, 0, z)) for z in (0, -8, 8, -6, 6, 0)], 0.5, False),
    }
    PROPS = {"keyboard": kb, "book": book, "bell": bell}
    r = math.radians
    import numpy as np, json
    manifest = {}
    for name, (seq, seconds, loop) in states.items():
        frames = []
        for i, p in enumerate(seq):
            head.rotation_euler = tuple(map(r, p["head"]))
            sh_l.rotation_euler = tuple(map(r, p["sl"])); sh_r.rotation_euler = tuple(map(r, p["sr"]))
            el_l.rotation_euler = tuple(map(r, p["el"])); el_r.rotation_euler = tuple(map(r, p["er"]))
            bell.rotation_euler = (0, -r(p["sr"][1] + p["er"][1]) + r(p.get("bell", 0)), 0)
            eb.inputs["Emission Strength"].default_value = p["glow"]
            for prop, root in PROPS.items():
                for o in [root] + list(root.children_recursive):
                    o.hide_render = prop not in p["props"]
            path = os.path.join(OUT, f"{name}_{i:02d}.png")
            scene.render.filepath = path
            bpy.ops.render.render(write_still=True)
            frames.append(path)
        # Join the frames side by side into one strip.
        imgs = [bpy.data.images.load(pth) for pth in frames]
        w, h = imgs[0].size
        strip = np.zeros((h, w * len(imgs), 4), dtype=np.float32)
        for k, im in enumerate(imgs):
            px = np.empty(w * h * 4, dtype=np.float32); im.pixels.foreach_get(px)
            strip[:, k * w:(k + 1) * w, :] = px.reshape(h, w, 4)
        out = bpy.data.images.new(f"strip_{name}", w * len(imgs), h, alpha=True)
        out.pixels.foreach_set(strip.ravel())
        out.filepath_raw = os.path.join(DEST, f"{name}.webp"); out.file_format = "WEBP"
        try:
            out.save(quality=90)
        except TypeError:
            out.save()
        manifest[name] = {"frames": len(imgs), "seconds": seconds, "loop": loop}
        L("strip", name, len(imgs))
    open(os.path.join(DEST, "strips.js"), "w").write(
        "// Generated by art/skeleton3d/build.py; do not edit. Frames per strip, seconds per pass, and\n"
        "// whether the state repeats (a state that plays once holds its last frame).\n"
        "export default " + json.dumps(manifest, indent=2) + ";\n")
    bpy.ops.wm.save_as_mainfile(filepath=os.path.join(OUT, "skeleton3d.blend"))
    L("saved")

try:
    main()
    open(os.path.join(OUT, "done.txt"), "w").write("ok")
except Exception:
    L(traceback.format_exc())
    open(os.path.join(OUT, "done.txt"), "w").write("error")
