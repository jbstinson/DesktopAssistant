# Builds the 3D skeleton avatar: an original skeleton bust modelled in Blender, rendered as frames
# per clip, then joined into one strip per clip for ui/avatars/skeleton3d/.
# Run (Blender 4.2+, headless):
#   blender --background --factory-startup --python art/skeleton3d/build.py -- ui/avatars/skeleton3d TEMPDIR
# Writes <clip>.webp strips and strips.js (frame counts and timing) into the first folder; the
# loose frames and the .blend file go to TEMPDIR. Blender's console is hidden in the Store build,
# so progress goes to TEMPDIR/log.txt and TEMPDIR/done.txt says "ok" or "error".
#
# Clips: one loop per app state (asleep, idle, typing, reading, thinking, done, needs-you, error),
# a one-off "<state>-in" transition from the idle pose into some of them, and one-off
# "fidget-*" clips the app plays now and then while idle. Done and error play once and hold.
import bpy, math, os, random, sys, traceback, json
from mathutils import Vector

ARGS = sys.argv[sys.argv.index("--") + 1:]
DEST, OUT = os.path.abspath(ARGS[0]), os.path.abspath(ARGS[1])
# An optional third argument renders just that clip, as loose frames only, for checking.
ONLY = ARGS[2] if len(ARGS) > 2 else None
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
    SLATE, _ = principled("Slate", (0.13, 0.17, 0.23), 0.5)
    KEYCAP, _ = principled("Keycap", (0.72, 0.77, 0.83), 0.4)
    PAGE, _ = principled("Page", (0.93, 0.91, 0.85), 0.8)
    COVER, _ = principled("Cover", (0.20, 0.06, 0.025), 0.5)
    INK, _ = principled("Ink", (0.35, 0.37, 0.42), 0.8)
    BRASS, brb = principled("Brass", (0.85, 0.62, 0.18), 0.3)
    brb.inputs["Metallic"].default_value = 1.0
    WOOD, _ = principled("Wood", (0.35, 0.18, 0.07), 0.6)

    # ---------- helpers ----------
    # Every mesh has its transform applied, so its vertices are in world space at rest and its
    # parent empties alone carry the pose.
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

    def ring(name, center, rx, ry, tilt, thick, m, parent):
        cu = bpy.data.curves.new(name, "CURVE"); cu.dimensions = "3D"
        cu.bevel_depth = thick; cu.bevel_resolution = 4
        sp = cu.splines.new("NURBS"); n = 28; sp.points.add(n - 1)
        for i in range(n):
            t = 2 * math.pi * i / n
            sp.points[i].co = (rx * math.cos(t), ry * math.sin(t), 0, 1)
        sp.use_cyclic_u = True; sp.order_u = 4
        o = bpy.data.objects.new(name, cu); o.location = center; o.rotation_euler = (math.radians(tilt), 0, 0)
        coll.objects.link(o); cu.materials.append(m)
        return finish(o, None, parent, False)

    # ---------- body: everything above the waist hangs off the torso, which leans at its base ----------
    torso = empty("torso", (0, 0.1, 0.25))
    for i, z in enumerate([0.26 + 0.088 * k for k in range(9)]):
        limb(f"vert{i}", (0, 0.1, z), (0, 0.1, z + 0.055), 0.05, BONE, torso, knobs=0)
    for i, (z, w) in enumerate([(0.79, 0.37), (0.67, 0.40), (0.55, 0.39), (0.43, 0.35), (0.32, 0.29)]):
        ring(f"rib{i}", (0, 0.03, z), w, 0.30, 16, 0.027, BONE, torso)
    rbox("sternum", (0, -0.27, 0.6), (0.045, 0.03, 0.2), BONE, torso, bevel=0.025)
    for s in (-1, 1):
        limb(f"clavicle{s}", (0.06 * s, -0.26, 0.86), (0.53 * s, -0.02, 0.82), 0.032, BONE, torso)

    def arm(side):
        sx = 0.57 * side
        sh = empty(f"shoulder{side}", (sx, 0, 0.8), torso)
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

    # ---------- head, with a jaw hinged near the ears ----------
    head = empty("head", (0, 0, 0.98), torso)
    jaw_pivot = empty("jawpivot", (0, -0.02, 1.1), head)
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
    finish(cranium, None, head); finish(face, None, head); finish(jaw, None, jaw_pivot)
    for row, z in ((0, 1.145), (1, 1.105)):
        for k in range(7):
            x = -0.15 + 0.05 * k
            rbox(f"tooth{row}{k}", (x, -0.405 + 0.6 * x * x, z), (0.02, 0.02, 0.026), BONE,
                 head if row == 0 else jaw_pivot, bevel=0.012)
    for i, z in enumerate((0.9, 0.97)):
        limb(f"neck{i}", (0, 0.05, z), (0, 0.05, z + 0.05), 0.055, BONE, torso, knobs=0)

    # ---------- keyboard (typing) ----------
    KB_Z = 0.274
    kb = empty("keyboard", (0, -0.66, KB_Z))
    rbox("kbbase", (0, -0.66, KB_Z), (0.78, 0.17, 0.03), SLATE, kb, bevel=0.02)
    for r_, y in enumerate((-0.73, -0.61)):
        for k in range(11):
            rbox(f"key{r_}{k}", (-0.70 + 0.14 * k, y, 0.312), (0.055, 0.045, 0.012), KEYCAP, kb, bevel=0.01)

    # ---------- book (reading): held the way a reader holds it, pages toward the skull, spine
    # toward the viewer, halves folded back to the hands, and laid back like a book read in the lap:
    # bottom toward the body, top away, pages facing up to the face, so a turning page rises up
    # where the viewer can see it ----------
    BOOK_Z = 0.66
    BOOK_TILT = 55
    book = empty("book", (0, -0.6, BOOK_Z))
    rbox("spine", (0, -0.635, 0.66), (0.03, 0.02, 0.35), COVER, book, bevel=0.012)
    for s in (-1, 1):
        half = empty(f"half{s}", (0, -0.62, 0.66), book)
        rbox(f"cover{s}", (0.26 * s, -0.62, 0.66), (0.26, 0.012, 0.35), COVER, half, bevel=0.01)
        rbox(f"page{s}", (0.25 * s, -0.596, 0.66), (0.24, 0.012, 0.33), PAGE, half, bevel=0.008)
        for k in range(6):
            rbox(f"line{s}{k}", (0.25 * s, -0.583, 0.86 - 0.075 * k), (0.17 if k % 3 else 0.12, 0.002, 0.008), INK, half, bevel=0.002)
        half.rotation_euler = (0, 0, math.radians(25 * s))
    # The page that turns: it starts on the reader's right half (the viewer's left) and swings
    # over the spine toward the skull to the other half.
    LEAF_Z = 0.66
    leaf = empty("leaf", (0, -0.62, LEAF_Z), book)
    rbox("leafpage", (-0.25, -0.577, 0.66), (0.235, 0.004, 0.32), PAGE, leaf, bevel=0.003)
    for k in range(5):
        rbox(f"leafline{k}", (-0.25, -0.572, 0.84 - 0.075 * k), (0.16, 0.002, 0.008), INK, leaf, bevel=0.002)
    book.rotation_euler = (math.radians(BOOK_TILT), 0, 0)

    # ---------- bell (done): hangs from the right hand; its pivot turns against the arm ----------
    bell = empty("bell", (0.57, 0, -0.17), el_r)
    limb("bellhandle", (0.57, 0, -0.17), (0.57, 0, -0.30), 0.026, WOOD, bell, knobs=0)
    bpy.ops.mesh.primitive_cone_add(vertices=48, radius1=0.19, radius2=0.06, depth=0.24, location=(0.57, 0, -0.42))
    finish(active(), BRASS, bell)
    sphere("clapper", (0.57, 0, -0.55), (0.04,) * 3, BRASS, bell, seg=24)

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
    scene.cycles.device = "CPU"; scene.cycles.samples = 48; scene.cycles.use_denoising = True
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

    # ---------- posing ----------
    r = math.radians
    PROPS = {"keyboard": kb, "book": book, "bell": bell}
    DEF = dict(torso=0.0, head=(0, 0, 0), jaw=(0, 0, 0), sl=(0, 8, 0), sr=(0, -8, 0), el=(0, 0, 0), er=(0, 0, 0),
               glow=5.0, props=(), bell=0.0, leaf=-25.0, book_z=BOOK_Z, kb_z=0.0)

    def pose(p):
        torso.rotation_euler = (r(p["torso"]), 0, 0)
        head.rotation_euler = tuple(map(r, p["head"])); jaw_pivot.rotation_euler = tuple(map(r, p["jaw"]))
        sh_l.rotation_euler = tuple(map(r, p["sl"])); sh_r.rotation_euler = tuple(map(r, p["sr"]))
        el_l.rotation_euler = tuple(map(r, p["el"])); el_r.rotation_euler = tuple(map(r, p["er"]))
        bell.rotation_euler = (0, -r(p["sr"][1] + p["er"][1]) + r(p["bell"]), 0)
        leaf.rotation_euler = (0, 0, r(p["leaf"]))
        # The turning page lifts out of the book mid-turn, so the viewer sees it rise over the top
        # edge; flat against a half it sits back down.
        turned = min(max((-25 - p["leaf"]) / 115, 0), 1)
        leaf.location.z = LEAF_Z + 0.32 * math.sin(math.pi * turned)
        book.location.z = p["book_z"]; kb.location.z = KB_Z + p["kb_z"]
        eb.inputs["Emission Strength"].default_value = p["glow"]
        for prop, root in PROPS.items():
            for o in [root] + list(root.children_recursive):
                o.hide_render = prop not in p["props"]

    def f(base, **kw):
        d = dict(base); d.update(kw); return d

    def ease(t):
        return t * t * (3 - 2 * t)

    def lerp(a, b, t):
        out = {}
        for k in a:
            if k == "props":
                out[k] = b[k]
            elif isinstance(a[k], tuple):
                out[k] = tuple(x + (y - x) * t for x, y in zip(a[k], b[k]))
            else:
                out[k] = a[k] + (b[k] - a[k]) * t
        return out

    def path(keys, n):
        """n frames through the key poses, evenly spaced in time, eased between each pair."""
        frames = []
        for i in range(n):
            t = i / (n - 1) * (len(keys) - 1)
            k = min(int(t), len(keys) - 2)
            frames.append(lerp(keys[k], keys[k + 1], ease(t - k)))
        return frames

    def cycle(n, fn):
        """n frames of a seamless loop; fn gets the phase 0..2pi."""
        return [fn(2 * math.pi * i / n) for i in range(n)]

    def flicker(frames, seed, amount=0.25):
        rng = random.Random(seed)
        return [f(p, glow=p["glow"] * (1 - amount / 2 + amount * rng.random())) for p in frames]

    # Find the typing arm angles that rest the middle fingertip on the front row of keys while the
    # torso leans in, so leaning never lifts the hands off the keyboard.
    tip_obj = bpy.data.objects["finger-11_end1"]
    tip_local = sum((v.co for v in tip_obj.data.vertices), Vector()) / len(tip_obj.data.vertices)
    tip_r = 0.013 * 1.35
    key_top = 0.312 + 0.012
    LEAN = 8
    def tip(a, b):
        pose(f(DEF, torso=LEAN, sl=(a, 0, 0), el=(b, 0, 0)))
        bpy.context.view_layer.update()
        return tip_obj.matrix_world @ tip_local
    best = None
    for a in range(-40, 6):
        for b in range(-100, -30):
            p = tip(a, b)
            err = (p.z - (key_top + tip_r)) ** 2 + (p.y + 0.73) ** 2
            if best is None or err < best[0]:
                best = (err, a, b, p)
    _, A0, B0, P0 = best

    # Reading holds the book low, leaving open space between its top edge and the chin where the
    # turning page shows. Find the arm angles that put the hands on the book's side edges.
    READ_Z = 0.5
    hand_obj = bpy.data.objects["hand-1"]
    hand_local = sum((v.co for v in hand_obj.data.vertices), Vector()) / len(hand_obj.data.vertices)
    # The middle of a half's outer edge, after the halves fold back 25 degrees and the book tilts.
    edge_y = -0.6 + 0.22 * math.cos(r(BOOK_TILT))
    edge_z = READ_Z + 0.22 * math.sin(r(BOOK_TILT))
    best = None
    for a in range(-60, 11, 2):
        for b in range(-150, -29, 2):
            pose(f(DEF, sl=(a, 0, 0), el=(b, 0, 0)))
            bpy.context.view_layer.update()
            q = hand_obj.matrix_world @ hand_local
            err = (q.y - edge_y) ** 2 + (q.z - edge_z) ** 2
            if best is None or err < best[0]:
                best = (err, a, b, q)
    _, RA, RB, Q = best
    L("reading arms", RA, RB, "hand", f"{Q.x:.3f} {Q.y:.3f} {Q.z:.3f}")
    L("typing arms", A0, B0, "tip", f"{P0.x:.3f} {P0.y:.3f} {P0.z:.3f}", "key top", key_top)

    # ---------- the clips: (frames, seconds per pass, repeats) ----------
    TYPE = f(DEF, torso=LEAN, head=(10, 0, 0), sl=(A0, 0, 0), sr=(A0, 0, 0), el=(B0, 0, 0), er=(B0, 0, 0),
             props=("keyboard",), jaw=(2, 0, 0))
    def typing(ph):
        s = math.sin(ph)
        # One hand presses (2 degrees less bend) while the other lifts (4 more), in turn.
        return f(TYPE, el=(B0 - 1 + 3 * s, 0, 0), er=(B0 - 1 - 3 * s, 0, 0),
                 head=(10 + 2 * abs(s), 0, 2 * s), jaw=(2 + 3 * abs(math.sin(2 * ph)), 0, 0))

    READ = f(DEF, sl=(RA, 0, 0), sr=(RA, 0, 0), el=(RB, 0, 0), er=(RB, 0, 0), props=("book",), head=(16, 0, 10), book_z=READ_Z)
    def reading():
        # Scan the reader's left page then the right (viewer's right to left), then turn the page
        # while the eyes go back to the top of the left page.
        scan = path([f(READ, head=(16, 0, 10)), f(READ, head=(18, 0, -8))], 6)
        turn = path([f(READ, head=(18, 0, -8), leaf=-25), f(READ, head=(16, 0, 10), leaf=-140)], 7)[:-1]
        return scan + turn

    THINK = lambda ph: f(DEF, head=(-8, 0, -18 * math.cos(ph)), jaw=(1.5 + 1.5 * math.sin(2 * ph), 0, 0))
    WAVE = f(DEF, head=(-8, 6, -15), sr=(0, -115, 0), glow=8)
    def waving(ph):
        return f(WAVE, er=(0, -65 + 30 * math.sin(ph), 0), head=(-8, 6 + 2 * math.sin(ph), -15),
                 jaw=(3 + 3 * abs(math.sin(ph)), 0, 0))
    RING = f(DEF, head=(0, -6, 6), sr=(0, -60, 0), er=(0, -130, 0), props=("bell",), glow=7)
    SCARED = f(DEF, sl=(0, 35, 0), sr=(0, -35, 0), jaw=(18, 0, 0), glow=13)
    ASLEEP = lambda ph: f(DEF, head=(18, 0, 10), jaw=(8 + 1.5 * math.sin(ph), 0, 0),
                          sl=(0, 7 + 2 * (0.5 - 0.5 * math.cos(ph)), 0), sr=(0, -7 - 2 * (0.5 - 0.5 * math.cos(ph)), 0),
                          glow=0.2 + 1.0 * (0.5 - 0.5 * math.cos(ph)))
    IDLE = lambda ph: f(DEF, torso=1.0 * (0.5 - 0.5 * math.cos(ph)), head=(2 * (0.5 - 0.5 * math.cos(ph)), 0, 0),
                        sl=(0, 8 + 2 * (0.5 - 0.5 * math.cos(ph)), 0), sr=(0, -8 - 2 * (0.5 - 0.5 * math.cos(ph)), 0))
    UP = f(DEF, sr=(0, -150, 0), er=(0, -65, 0), head=(0, -8, 6))

    clips = {
        # Loops, one per app state.
        "asleep": (cycle(8, ASLEEP), 4.0, True),
        "idle": (flicker(cycle(12, IDLE), "idle", 0.15), 3.0, True),
        "typing": (flicker(cycle(8, typing), "typing"), 0.6, True),
        "reading": (flicker(reading(), "reading", 0.15), 3.2, True),
        "thinking": (flicker(cycle(20, THINK), "thinking"), 3.0, True),
        "needs-you": (flicker(cycle(16, waving), "needs-you"), 1.4, True),
        # Played once, holding the last frame.
        "done": (path([DEF, RING], 4) + [f(RING, bell=b, jaw=(j, 0, 0)) for b, j in
                 ((0, 0), (-12, 6), (-22, 9), (0, 4), (18, 10), (4, 5), (-12, 8), (-4, 3), (6, 4), (0, 0))], 1.8, False),
        "error": (flicker(path([DEF, SCARED], 3) + [f(SCARED, head=(0, 0, z)) for z in (-8, 8, -6, 6, -3, 0)], "error", 0.4),
                  0.7, False),
        # Transitions from the idle pose into a state's loop.
        "typing-in": (path([f(DEF, kb_z=-0.5, props=("keyboard",)), typing(0)], 6), 0.5, False),
        "reading-in": (path([f(DEF, props=("book",), book_z=0.1), reading()[0]], 8), 0.8, False),
        "thinking-in": (path([DEF, THINK(0)], 5), 0.5, False),
        "needs-you-in": (path([DEF, f(WAVE, sr=(0, -40, 0), er=(0, -20, 0)), waving(0)], 8), 0.7, False),
        "asleep-in": (path([DEF, f(ASLEEP(0), head=(10, 0, 4), glow=2.0), ASLEEP(0)], 10), 1.6, False),
        # Fidgets, now and then while idle.
        "fidget-look": (path([DEF, f(DEF, head=(-2, 0, -25)), f(DEF, head=(-2, 0, -25)), f(DEF, head=(-2, 0, 25)),
                              f(DEF, head=(-2, 0, 25)), DEF], 14), 2.0, False),
        "fidget-scratch": (path([DEF, UP, f(UP, er=(0, -58, 0)), f(UP, er=(0, -72, 0)), f(UP, er=(0, -58, 0)),
                                 f(UP, er=(0, -72, 0)), f(UP, er=(0, -58, 0)), UP, DEF], 16), 2.2, False),
        "fidget-jaw": (path([DEF, f(DEF, jaw=(12, 0, 0)), f(DEF, jaw=(10, 0, 6), head=(0, 3, 0)),
                             f(DEF, jaw=(10, 0, -6), head=(0, -3, 0)), DEF], 10), 1.4, False),
    }

    # ---------- render each clip and join its frames into one strip ----------
    import numpy as np
    manifest = {}
    for name, (seq, seconds, loop) in clips.items():
        if ONLY and name != ONLY:
            continue
        paths = []
        for i, p in enumerate(seq):
            pose(p)
            if ONLY:
                bpy.context.view_layer.update()
                lp = bpy.data.objects["leafpage"]
                zs = [(lp.matrix_world @ v.co).z for v in lp.data.vertices]
                cv = bpy.data.objects["cover-1"]
                cz = [(cv.matrix_world @ v.co).z for v in cv.data.vertices]
                L(name, i, f"leaf {p['leaf']:.0f}", f"page top {max(zs):.3f}", f"cover top {max(cz):.3f}", "hidden", lp.hide_render)
            pth = os.path.join(OUT, f"{name}_{i:02d}.png")
            scene.render.filepath = pth
            bpy.ops.render.render(write_still=True)
            paths.append(pth)
        if ONLY:
            break
        imgs = [bpy.data.images.load(pth) for pth in paths]
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
        for im in imgs: bpy.data.images.remove(im)
        bpy.data.images.remove(out)
        manifest[name] = {"frames": len(seq), "seconds": seconds, "loop": loop}
        L("strip", name, len(seq))
    if ONLY:
        return
    # Strips from an earlier build that are no longer clips would linger, so remove them.
    for fn in os.listdir(DEST):
        if fn.endswith(".webp") and fn[:-5] not in manifest:
            os.remove(os.path.join(DEST, fn))
    open(os.path.join(DEST, "strips.js"), "w", newline="\n").write(
        "// Generated by art/skeleton3d/build.py; do not edit. Frames per strip, seconds per pass, and\n"
        "// whether the clip repeats (a clip that plays once holds its last frame).\n"
        "export default " + json.dumps(manifest, indent=2) + ";\n")
    bpy.ops.wm.save_as_mainfile(filepath=os.path.join(OUT, "skeleton3d.blend"))
    L("saved")

try:
    main()
    open(os.path.join(OUT, "done.txt"), "w").write("ok")
except Exception:
    L(traceback.format_exc())
    open(os.path.join(OUT, "done.txt"), "w").write("error")
