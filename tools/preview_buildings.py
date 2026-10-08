"""Blender contact sheet of only the generated kit, no original-game assets.

blender --background --factory-startup --python tools/preview_buildings.py -- building-kit.png
Optionally append --blend building-kit.blend to save a review scene (not committed: regenerate it).
"""

import argparse
import math
from pathlib import Path
import sys

import bpy
from mathutils import Matrix, Vector

sys.path.insert(0, str(Path(__file__).resolve().parent))
from generate_buildings import NAMES


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    parser.add_argument("--blend", type=Path)
    args = parser.parse_args(sys.argv[sys.argv.index("--")+1:])
    root = Path(__file__).resolve().parents[1]
    bpy.ops.object.select_all(action="SELECT")
    bpy.ops.object.delete(use_global=False)
    scene = bpy.context.scene
    for i, name in enumerate(NAMES):
        before = set(bpy.data.objects)
        bpy.ops.import_scene.gltf(filepath=str(root / "assets/3d/buildings" / f"{name}.glb"))
        imported = set(bpy.data.objects) - before
        offset = Vector(((i % 4 - 1.5)*4.6, (1.5 - i//4)*4.8, 0))
        for obj in imported:
            if obj.type == "MESH" and obj.name.startswith("Scaffold"):
                bpy.data.objects.remove(obj, do_unlink=True)
            elif obj.parent is None:
                # glTF -Z (entry) imports as Blender +Y; face the entry toward our camera.
                obj.matrix_world = Matrix.Rotation(math.pi, 4, "Z") @ obj.matrix_world
                obj.location += offset
        # Each plinth and caption is review furniture, not part of the shipped model.
        bpy.ops.mesh.primitive_cube_add(size=1, location=offset+Vector((0,0,-.13)))
        plinth = bpy.context.object
        plinth.scale = (4.15,4.25,.2)
        plinth.color = (.17,.23,.24,1)
        bpy.ops.object.text_add(location=offset+Vector((0,-1.9,.015)))
        text = bpy.context.object
        text.data.body = name.replace("_", " ").upper()
        text.data.align_x = "CENTER"
        text.data.size = .22
        text.color = (.9,.92,.89,1)

    bpy.ops.object.camera_add(location=(8,-15,24))
    camera = bpy.context.object
    camera.rotation_euler = (Vector((0,0,.3))-camera.location).to_track_quat("-Z","Y").to_euler()
    camera.data.type = "ORTHO"
    camera.data.ortho_scale = 26
    scene.camera = camera
    # Render the actual glTF textures/material factors, not Workbench's flat viewport colours.
    scene.render.engine = "CYCLES"
    scene.cycles.device = "CPU"
    scene.cycles.samples = 24
    scene.cycles.use_denoising = True
    scene.world.use_nodes = True
    background = scene.world.node_tree.nodes.get("Background")
    background.inputs["Color"].default_value = (.32,.39,.48,1)
    background.inputs["Strength"].default_value = .7
    for at, energy, size in (((2,-6,16),2500,10),((-8,3,12),1700,8)):
        bpy.ops.object.light_add(type="AREA", location=at)
        light = bpy.context.object
        light.data.energy = energy
        light.data.shape = "DISK"
        light.data.size = size
        light.rotation_euler = (-light.location).to_track_quat("-Z","Y").to_euler()
    # Material colours for the review furniture.
    for obj in scene.objects:
        if obj.type in {"MESH", "FONT"} and not obj.data.materials:
            mat = bpy.data.materials.new("Review")
            mat.diffuse_color = obj.color
            mat.use_nodes = True
            mat.node_tree.nodes.get("Principled BSDF").inputs["Base Color"].default_value = obj.color
            obj.data.materials.append(mat)
    scene.render.resolution_x = 1800
    scene.render.resolution_y = 1800
    scene.render.resolution_percentage = 100
    scene.render.image_settings.file_format = "PNG"
    scene.render.filepath = str(args.output.resolve())
    args.output.parent.mkdir(parents=True, exist_ok=True)
    if args.blend:
        args.blend.parent.mkdir(parents=True, exist_ok=True)
        bpy.ops.wm.save_as_mainfile(filepath=str(args.blend.resolve()))
    bpy.ops.render.render(write_still=True)


if __name__ == "__main__":
    main()
