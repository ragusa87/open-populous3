#!/usr/bin/env python3
"""Generated Open Populous building kit. No image inputs or third-party geometry.

Python 3 standard library; writes deterministic glTF 2.0 binary assets. Import a GLB
into Blender to inspect/edit it. Code: MIT; generated artwork: CC0-1.0.
Coordinates: cells, Y up, door -Z. Scene 0 "Built" = Body mesh, scene 1 "Construction" = Scaffold.
"""

import argparse
import json
import math
from pathlib import Path
import struct

from building_textures import PALETTE, atlas_png, atlas_uv, png_pixels

NAMES = (
    "hut_small", "hut_medium", "hut_large", "drum_tower", "temple", "spy_hut",
    "warrior_hut", "firewarrior_hut", "reconversion", "wall", "gate", "boat_hut",
    "airship_hut", "guard_post", "vault", "prison",
)


def add(a, b):
    return tuple(x + y for x, y in zip(a, b))


def sub(a, b):
    return tuple(x - y for x, y in zip(a, b))


def mul(a, f):
    return tuple(x * f for x in a)


def cross(a, b):
    return (a[1]*b[2]-a[2]*b[1], a[2]*b[0]-a[0]*b[2], a[0]*b[1]-a[1]*b[0])


def dot(a, b):
    return sum(x*y for x, y in zip(a, b))


def unit(a):
    return mul(a, 1 / math.sqrt(dot(a, a)))


class Mesh:
    def __init__(self):
        self.parts = {}

    def face(self, points, material, inside, grain=(0, 1, 0)):
        points = list(points)
        n = unit(cross(sub(points[1], points[0]), sub(points[2], points[0])))
        if dot(n, sub(points[0], inside)) < 0:
            points.reverse()
            n = mul(n, -1)
        # Project onto the face, aligning straw/wood fibres with the supplied direction.
        along = sub(grain, mul(n, dot(grain, n)))
        if dot(along, along) < 1e-8:
            along = sub(points[1], points[0])
        v = unit(along)
        u = unit(cross(v, n))
        projected = [(dot(p, u), dot(p, v)) for p in points]
        lo = [min(p[i] for p in projected) for i in (0, 1)]
        span = [max(p[i] for p in projected) - lo[i] for i in (0, 1)]
        # Isotropic density, one swatch per 1.5 cells; fit larger faces without atlas wrapping.
        scale = max(1.5, *span)
        texcoords = [atlas_uv(material, (p[0]-lo[0])/scale, (p[1]-lo[1])/scale) for p in projected]
        positions, normals, uvs = self.parts.setdefault(material, ([], [], []))
        for i in range(1, len(points) - 1):
            positions.extend((points[0], points[i], points[i + 1]))
            normals.extend((n, n, n))
            uvs.extend((texcoords[0], texcoords[i], texcoords[i + 1]))

    def box(self, at, size, material="Wood"):
        x, y, z = (v/2 for v in size)
        p = [add(at, v) for v in ((-x,-y,-z),(x,-y,-z),(x,y,-z),(-x,y,-z),
                                  (-x,-y,z),(x,-y,z),(x,y,z),(-x,y,z))]
        for f in ((0,1,2,3),(4,7,6,5),(0,4,5,1),(3,2,6,7),(0,3,7,4),(1,5,6,2)):
            self.face([p[i] for i in f], material, at)

    def beam(self, a, b, width=.07, material="Timber"):
        d = unit(sub(b, a))
        u = mul(unit(cross(d, (1,0,0) if abs(d[1]) > .9 else (0,1,0))), width/2)
        v = mul(unit(cross(d, u)), width/2)
        ring = lambda p: [add(p, add(mul(u, i), mul(v, j))) for i,j in ((-1,-1),(1,-1),(1,1),(-1,1))]
        ra, rb = ring(a), ring(b)
        centre = mul(add(a, b), .5)
        for i in range(4):
            j = (i+1) % 4
            self.face((ra[i], ra[j], rb[j], rb[i]), material, centre, grain=d)
        self.face(ra, material, centre)
        self.face(rb, material, centre)

    def round(self, at, radius, top_radius, height, material, sides=12):
        """Capped frustum; all radii nonzero (no degenerate apex triangles)."""
        x, y, z = at
        ring = lambda r,h: [(x+r*math.sin(i*math.tau/sides), y+h, z+r*math.cos(i*math.tau/sides)) for i in range(sides)]
        lo, hi = ring(radius, 0), ring(top_radius, height)
        centre = (x,y+height/2,z)
        for i in range(sides):
            j = (i+1) % sides
            self.face((lo[i],lo[j],hi[j],hi[i]), material, centre)
        self.face(lo, material, centre)
        self.face(hi, material, centre)

    def roof(self, x, z, half_x, half_z, eave, peak, material="Thatch"):
        """Solid triangular gable prism, ridge along Z."""
        a,b,c = (x-half_x,eave,z-half_z),(x+half_x,eave,z-half_z),(x,peak,z-half_z)
        d,e,f = (x-half_x,eave,z+half_z),(x+half_x,eave,z+half_z),(x,peak,z+half_z)
        inside = (x, eave+(peak-eave)/3, z)
        for face in ((a,b,c),(d,f,e),(a,c,f,d),(b,e,f,c),(a,d,e,b)):
            self.face(face, material, inside)


class Building:
    def __init__(self):
        self.body, self.frame = Mesh(), Mesh()

    def post(self, a, b, width=.08):
        self.body.beam(a, b, width, "Wood")
        self.frame.beam(a, b, width*.8)

    def lodge(self, x, z, hx, hz, wall, peak, dark=False, open_sides=False):
        m = self.body
        m.box((x,.06,z), (hx*2,.12,hz*2), "Stone")
        if not open_sides:
            m.box((x,(wall+.12)/2,z), (hx*1.85,wall-.12,hz*1.85), "Dark" if dark else "Clay")
            # A recessed dark opening under a projecting timber lintel.
            m.box((x,.38,z-hz*.94), (.38,.56,.06), "Dark")
            for dx in (-.24,.24):
                self.post((x+dx,.12,z-hz), (x+dx,.72,z-hz), .075)
            self.post((x-.28,.74,z-hz), (x+.28,.74,z-hz), .08)
        for dx in (-hx*.88,hx*.88):
            for dz in (-hz*.88,hz*.88):
                self.post((x+dx,.1,z+dz),(x+dx,wall,z+dz))
        for dz in (-hz*.88,hz*.88):
            self.post((x-hx*.88,wall,z+dz),(x+hx*.88,wall,z+dz))
            self.frame.beam((x-hx,wall,z+dz),(x,peak,z+dz))
            self.frame.beam((x+hx,wall,z+dz),(x,peak,z+dz))
        self.frame.beam((x,peak,z-hz),(x,peak,z+hz))
        m.roof(x,z,hx,hz,wall,peak,"Dark" if dark else "Thatch")
        # A broad central cloth strip reads from distant/aerial cameras.
        m.roof(x,z,hx*.25,hz+.012,peak-(peak-wall)*.25+.018,peak+.018,"Tribe")
        m.box((x,.88,z-hz-.025), (.35,.18,.035), "Tribe")

    def hut(self, size):
        m = self.body
        r, wall, peak = (.66+.13*size, .57+.11*size, 1.16+.18*size)
        m.round((0,0,0),r,r,.12,"Stone")
        m.round((0,.12,0),r*.9,r*.86,wall-.12,"Clay")
        m.round((0,wall-.15,0),r*.89,r*.89,.12,"Tribe")
        # Three overlapping thatch courses, not a smooth featureless cone.
        for i in range(3):
            t = i/3
            rr = r*1.09*(1-t)+.07*t
            rt = r*1.09*(1-(i+1)/3)+.07*(i+1)/3
            m.round((0,wall+(peak-wall)*t,0),rr,rt,(peak-wall)/3+.018,"Thatch" if i%2 == 0 else "ThatchShade")
        # Coloured roof binding remains visible at an aerial camera angle.
        t, band = .55, .085
        radius_at = lambda t: r*1.09*(1-t)+.07*t
        m.round((0,wall+(peak-wall)*t+.022,0),radius_at(t)+.012,
                radius_at(t+band)+.012,(peak-wall)*band,"Tribe")
        self.lodge(0,-r*.72,.3,.3,.58,.84,open_sides=True)
        m.box((0,.35,-r*.88),(.39,.53,.035),"Dark")
        for i in range(8):
            a = i*math.tau/8
            x,z = math.sin(a)*r*.86,math.cos(a)*r*.86
            self.post((x,.1,z),(x,wall,z),.045)
            self.frame.beam((x,wall,z),(0,peak,0),.045)
        # Highest point is the smoke vent; the runtime's chimney() finds it.
        m.round((0,peak,0),.105,.09,.12,"Wood",8)
        if size >= 2:
            m.box((r*.77,.4,0),(.08,.26,.28),"Dark")
            m.box((r*.82,.55,0),(.14,.06,.38),"Linen")
        if size == 3:
            m.box((-r*.77,.4,0),(.08,.26,.28),"Dark")
            m.box((-r*.82,.55,0),(.14,.06,.38),"Linen")

    def tower(self):
        m = self.body
        for x in (-.38,.38):
            for z in (-.38,.38):
                self.post((x*1.2,0,z*1.2),(x,1.68,z),.12)
        for z in (-.38,.38):
            self.post((-.4,.25,z),(.38,1.25,z),.07)
        m.box((0,1.3,0),(1.14,.13,1.14),"Timber")
        for x in (-.52,.52):
            m.box((x,1.57,0),(.06,.35,1.05),"Tribe")
        m.roof(0,0,.64,.64,1.98,2.45)
        for z in (-.43,.43):
            for x in (-.43,.43):
                self.post((x,1.3,z),(x,2.0,z),.07)
        for y in (.2,.4,.6,.8,1.0,1.2):
            m.beam((-.16,y,-.46),(.16,y,-.46),.04)
        for x in (-.19,.19):
            m.beam((x,0,-.54),(x,1.3,-.43),.05)
        m.round((0,1.38,0),.23,.23,.32,"Wood",10)
        m.round((0,1.70,0),.25,.25,.035,"Linen",10)


def make(name):
    b = Building()
    m = b.body
    if name.startswith("hut_"):
        b.hut(NAMES.index(name)+1)
    elif name == "drum_tower":
        b.tower()
    elif name == "temple":
        for i in range(3):
            m.box((0,.09+i*.16,-.25),(2.7-i*.22,.18,3.05-i*.3),"Stone")
        b.lodge(0,-.1,1.06,1.15,1.05,1.7)
        m.roof(0,.18,.65,.7,1.68,2.22,"Tribe")
        for x in (-1.12,1.12):
            b.post((x,0,-1.48),(x,1.35,-1.48),.13)
            m.round((x,1.35,-1.48),.15,.035,.3,"Linen",6)
    elif name == "spy_hut":
        b.lodge(0,0,.67,.66,.73,1.22,dark=True)
        m.box((.36,.36,-.64),(.1,.09,.05),"Linen")
    elif name in ("warrior_hut", "firewarrior_hut"):
        fire = name == "firewarrior_hut"
        b.lodge(0,.23,.88 if not fire else 1.14,.82, .9,1.48)
        if fire:
            for x in (-1.08,1.08):
                m.round((x,0,-1.03),.16,.12,.62,"Stone",8)
                m.round((x,.62,-1.03),.25,.27,.14,"Dark",8)
                m.round((x,.76,-1.03),.16,.025,.29,"Ember",7)
        else:
            for a,c in (((-.45,.68,-.68),(.45,1.7,-.68)),((.45,.68,-.7),(-.45,1.7,-.7))):
                m.beam(a,c,.075,"Wood")
            m.round((.68,0,-.86),.14,.14,.48,"Timber",8)
    elif name == "boat_hut":
        b.lodge(0,-.16,1.06,.75,.92,1.5,open_sides=True)
        # Open launching channel, two piers extending toward local +Z.
        for x in (-.74,.74):
            for i in range(9):
                m.box((x,.11,.6+i*.17),(.46,.1,.15),"Timber")
            for z in (.8,1.9):
                b.post((x,0,z),(x,.42,z),.1)
    elif name == "airship_hut":
        b.lodge(-.35,0,.88,1.12,.98,1.55)
        m.box((1.48,.09,0),(1.8,.18,2.25),"Timber")
        for z in (-.88,.88):
            for x in (.78,2.18):
                b.post((x,.14,z),(x,2.45,z),.11)
            b.post((.7,2.45,z),(2.28,2.45,z),.14)
        m.box((1.49,2.48,0),(.1,.09,2.05),"Tribe")
        # Folded cloth on the platform, not a finished vehicle.
        for i in range(4):
            m.box((1.46,.23+i*.08,.25),(.86-i*.08,.07,.8),"Linen" if i%2 else "Tribe")
    elif name in ("vault", "reconversion"):
        r = 1.0 if name == "vault" else .73
        for i in range(4):
            m.box((0,.12+i*.21,.06),(2*r-i*.27,.24,2*r-i*.27),"Stone")
        m.round((0,.96,.06),.3,.05,.56,"Linen",4)
        if name == "reconversion":
            for x in (-.55,.55):
                b.post((x,.2,0),(x,1.45,0),.12)
            b.post((-.6,1.45,0),(.6,1.45,0),.14)
        else:
            m.box((0,.36,-.92),(.38,.48,.05),"Dark")
            m.box((0,.64,-.92),(.48,.09,.07),"Tribe")
    elif name == "prison":
        m.box((.06,.09,.06),(2,.18,2),"Stone")
        for x in (-.8,.8):
            for z in (-.8,.8):
                b.post((x,.15,z),(x,1.25,z),.15)
        for i in range(9):
            p = -.8+i*.2
            for x,z in ((p,-.83),(p,.83),(-.83,p),(.83,p)):
                m.beam((x,.15,z),(x,1.24,z),.045,"Dark")
        m.roof(0,0,1,1,1.25,1.59,"Stone")
        m.box((0,1.26,-1.01),(.34,.18,.04),"Tribe")
    elif name in ("wall", "gate"):
        if name == "wall":
            xs = [i*.18-.63 for i in range(8)]
        else:
            xs = [-.65,-.5,.5,.65]
            b.post((-.7,1.1,0),(.7,1.1,0),.18)
            m.box((0,1.12,-.12),(.32,.25,.05),"Tribe")
        for x in xs:
            h = 1.0 if name == "wall" else 1.25
            m.round((x,0,0),.115,.115,h,"Timber",6)
            m.round((x,h,0),.115,.015,.18,"Wood",6)
        if name == "wall":
            for y in (.25,.7):
                b.post((-.73,y,-.12),(.73,y,-.12),.09)
    elif name == "guard_post":
        m.round((0,0,0),.53,.53,.12,"Stone",8)
        b.post((0,.1,0),(0,1.55,0),.11)
        m.box((.24,1.29,0),(.48,.4,.04),"Tribe")
        m.round((0,1.55,0),.13,.025,.22,"Linen",6)
    else:
        raise ValueError(name)
    # Non-buildable landmarks also carry a valid frame for future editor use.
    if not b.frame.parts:
        for x in (-.6,.6):
            for z in (-.6,.6):
                b.frame.beam((x,0,z),(x,.8,z))
    return b


def linear(hex_colour):
    srgb = [int(hex_colour[i:i+2],16)/255 for i in (0,2,4)]
    return [v/12.92 if v <= .04045 else ((v+.055)/1.055)**2.4 for v in srgb]+[1]


def glb(building):
    data = bytearray()
    doc = {
        "asset": {"version":"2.0", "generator":"Open Populous generate_buildings.py", "copyright":"CC0-1.0"},
        "scene":0, "scenes":[{"name":"Built","nodes":[0]},{"name":"Construction","nodes":[1]}],
        "nodes":[{"name":"Body","mesh":0},{"name":"Scaffold","mesh":1}],
        "meshes":[], "accessors":[], "bufferViews":[],
        "materials":[{"name":k,"pbrMetallicRoughness":{"baseColorFactor":linear(v),"baseColorTexture":{"index":0},"metallicFactor":0,"roughnessFactor":.9}} for k,v in PALETTE.items()],
        "textures":[{"source":0,"sampler":0}],
        "samplers":[{"magFilter":9729,"minFilter":9987,"wrapS":33071,"wrapT":33071}],
    }

    def accessor(values):
        offset = len(data)
        dimensions = len(values[0])
        for v in values:
            data.extend(struct.pack(f"<{dimensions}f",*v))
        view = len(doc["bufferViews"])
        doc["bufferViews"].append({"buffer":0,"byteOffset":offset,"byteLength":len(data)-offset,"target":34962})
        result = len(doc["accessors"])
        doc["accessors"].append({"bufferView":view,"componentType":5126,"count":len(values),"type":f"VEC{dimensions}",
                                 "min":[min(v[i] for v in values) for i in range(dimensions)],"max":[max(v[i] for v in values) for i in range(dimensions)]})
        return result

    for label,mesh in (("Body",building.body),("Scaffold",building.frame)):
        primitives = []
        for mat,(positions,normals,uvs) in mesh.parts.items():
            primitives.append({"attributes":{"POSITION":accessor(positions),"NORMAL":accessor(normals),"TEXCOORD_0":accessor(uvs)},"mode":4,"material":list(PALETTE).index(mat)})
        doc["meshes"].append({"name":label,"primitives":primitives})
    # Linked, not embedded: the atlas sits next to every GLB, and the client bundles it once.
    doc["images"] = [{"name":"Building surfaces","uri":"surfaces.png","mimeType":"image/png"}]
    doc["buffers"] = [{"byteLength":len(data)}]
    text = json.dumps(doc,separators=(",",":"),sort_keys=True).encode()
    text += b" "*((-len(text))%4)
    data += b"\0"*((-len(data))%4)
    return struct.pack("<III",0x46546c67,2,28+len(text)+len(data))+struct.pack("<II",len(text),0x4e4f534a)+text+struct.pack("<II",len(data),0x004e4942)+data


def glb_content(glb_bytes):
    """The JSON document and every float of the binary chunk of a GLB written by `glb`."""
    text_length = struct.unpack_from("<I", glb_bytes, 12)[0]
    data = glb_bytes[28 + text_length:]
    return json.loads(glb_bytes[20:20 + text_length]), struct.unpack(f"<{len(data) // 4}f", data[:len(data) // 4 * 4])


def close(a, b, tolerance=1e-5):
    """Same structure and strings, floats within `tolerance`: libm results may differ by an ulp."""
    if isinstance(a, dict):
        return isinstance(b, dict) and a.keys() == b.keys() and all(close(a[k], b[k], tolerance) for k in a)
    if isinstance(a, (list, tuple)):
        return isinstance(b, (list, tuple)) and len(a) == len(b) and all(close(x, y, tolerance) for x, y in zip(a, b))
    if isinstance(a, float) or isinstance(b, float):
        return isinstance(a, (int, float)) and isinstance(b, (int, float)) and abs(a - b) <= tolerance
    return a == b


def same_glb(a, b):
    """Two GLBs with the same document and geometry, ignoring float rounding and byte layout."""
    return close(glb_content(a), glb_content(b))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check",action="store_true",help="fail if checked-in atlas pixels or GLB contents differ (floats within 1e-5); write nothing")
    parser.add_argument("--out",type=Path,default=Path(__file__).resolve().parents[1]/"assets/models/buildings")
    args = parser.parse_args()
    if not args.check:
        args.out.mkdir(parents=True,exist_ok=True)
    texture = atlas_png()
    atlas = args.out / "surfaces.png"
    if args.check:
        # Pixels, not bytes: deflate output differs between zlib builds.
        if not atlas.exists() or png_pixels(atlas.read_bytes()) != png_pixels(texture):
            raise SystemExit(f"Out of date: {atlas}")
    else:
        atlas.write_bytes(texture)
    for name in NAMES:
        content = glb(make(name))
        path = args.out/f"{name}.glb"
        if args.check:
            if not path.exists() or not same_glb(path.read_bytes(), content):
                raise SystemExit(f"Out of date: {path}")
        else:
            path.write_bytes(content)
        print(f"{name}: {len(content):,} bytes")


if __name__ == "__main__":
    main()
