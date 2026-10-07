//! Generated building kit, made by tools/generate_buildings.py, embedded for synchronous stage
//! rendering. No install discovery or original data. GLBs are also directly editable in Blender.
//! Contract: named Body/Scaffold meshes, baked cell-space positions, triangle primitives with
//! normals, atlas UVs and material colours. Only the material named Tribe follows the owner.

use crate::original_models::MeshData;
use crate::sites::tribe_color;
use bevy::asset::RenderAssetUsages;
use bevy::color::ColorToComponents;
use bevy::image::{CompressedImageFormats, Image, ImageFilterMode, ImageSampler, ImageSamplerDescriptor, ImageType};
use bevy::render::render_resource::TextureFormat;
use game_core::building::BuildingKind;
use std::sync::OnceLock;

const ATLAS: &[u8] = include_bytes!("../../../assets/models/buildings/surfaces.png");

/// One shared grayscale surface atlas, multiplied by each vertex's material/tribe pigment.
/// Four reductions keep distant roofs stable; tile gutters still cover half a texel at mip 4.
pub fn surface_image() -> Image {
    let sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        lod_max_clamp: 4.0,
        ..Default::default()
    });
    let mut image =
        Image::from_buffer(ATLAS, ImageType::Extension("png"), CompressedImageFormats::NONE, true, sampler, RenderAssetUsages::default()).expect("bundled building surface PNG");
    let size = image.texture_descriptor.size;
    assert!(size.width == size.height && size.width.is_power_of_two() && size.width >= 32, "building atlas must be square, a power of two");
    assert_eq!(image.texture_descriptor.format, TextureFormat::Rgba8UnormSrgb, "building atlas must be sRGB RGBA");
    let data = image.data.as_mut().expect("building surface pixels");
    let mut level = data.clone();
    let mut side = size.width as usize;
    for _ in 0..4 {
        level = half_gray(&level, side);
        data.extend_from_slice(&level);
        side /= 2;
    }
    image.texture_descriptor.mip_level_count = 5;
    image
}

/// Next mip level of a square grayscale RGBA8 sRGB image (red channel read), each texel the mean
/// of its 2x2 parents in linear light so mip transitions keep the pigment's brightness.
fn half_gray(level: &[u8], side: usize) -> Vec<u8> {
    let linear = |v: u8| {
        let v = v as f32 / 255.0;
        if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
    };
    let mut next = Vec::with_capacity(side * side);
    for y in 0..side / 2 {
        for x in 0..side / 2 {
            let value = [(0, 0), (1, 0), (0, 1), (1, 1)].into_iter().map(|(dx, dy)| linear(level[((2 * y + dy) * side + 2 * x + dx) * 4])).sum::<f32>() / 4.0;
            let srgb = if value <= 0.0031308 { value * 12.92 } else { 1.055 * value.powf(1.0 / 2.4) - 0.055 };
            let v = (srgb * 255.0).round() as u8;
            next.extend_from_slice(&[v, v, v, 255]);
        }
    }
    next
}

/// One kit building: the finished `Body` and its timber `Scaffold`, in cells, with vertex colours.
struct Model {
    body: MeshData,
    scaffold: MeshData,
}

/// Every kind the kit draws, one bundled model each, in `MODELS` order.
pub const KINDS: [BuildingKind; 16] = [
    BuildingKind::Hut { size: 1 },
    BuildingKind::Hut { size: 2 },
    BuildingKind::Hut { size: 3 },
    BuildingKind::DrumTower,
    BuildingKind::Temple,
    BuildingKind::SpyTraining,
    BuildingKind::WarriorTraining,
    BuildingKind::FirewarriorTraining,
    BuildingKind::BoatHut,
    BuildingKind::AirshipHut,
    BuildingKind::Vault,
    BuildingKind::Prison,
    BuildingKind::Reconversion,
    BuildingKind::WallPiece,
    BuildingKind::Gate,
    BuildingKind::GuardPost,
];

macro_rules! asset {
    ($name:literal) => {
        include_bytes!(concat!("../../../assets/models/buildings/", $name, ".glb")) as &[u8]
    };
}

const MODELS: [&[u8]; 16] = [
    asset!("hut_small"),
    asset!("hut_medium"),
    asset!("hut_large"),
    asset!("drum_tower"),
    asset!("temple"),
    asset!("spy_hut"),
    asset!("warrior_hut"),
    asset!("firewarrior_hut"),
    asset!("boat_hut"),
    asset!("airship_hut"),
    asset!("vault"),
    asset!("prison"),
    asset!("reconversion"),
    asset!("wall"),
    asset!("gate"),
    asset!("guard_post"),
];

/// The kit model drawing a kind, None for unknown model IDs. Huts of any other size are large.
fn index(kind: BuildingKind) -> Option<usize> {
    let kind = match kind {
        BuildingKind::Hut { size } if !(1..=2).contains(&size) => BuildingKind::Hut { size: 3 },
        kind => kind,
    };
    KINDS.iter().position(|&k| k == kind)
}

/// The kind's finished building with its `Tribe` material in the owner's colour, None for
/// unknown IDs. Each GLB is decoded once. Fail back to the diagnostic box if a future asset edit
/// breaks it; tests validate every shipped model before release.
pub fn body(kind: BuildingKind, owner: u8) -> Option<MeshData> {
    let d = decoded(kind)?;
    Some(painted(&d.model.body, &d.tribe[0], owner))
}

/// The kind's timber construction frame, in the owner's colour like `body`.
pub fn scaffold(kind: BuildingKind, owner: u8) -> Option<MeshData> {
    let d = decoded(kind)?;
    Some(painted(&d.model.scaffold, &d.tribe[1], owner))
}

fn decoded(kind: BuildingKind) -> Option<&'static Decoded> {
    static KIT: OnceLock<Vec<Option<Decoded>>> = OnceLock::new();
    KIT.get_or_init(|| MODELS.iter().map(|bytes| decode(bytes)).collect())[index(kind)?].as_ref()
}

/// A decoded model and, per mesh, which vertices take the owner's colour.
struct Decoded {
    model: Model,
    tribe: [Vec<bool>; 2],
}

/// A copy of `mesh` with the vertices flagged in `tribe` in the owner's colour.
fn painted(mesh: &MeshData, tribe: &[bool], owner: u8) -> MeshData {
    let colour = tribe_color(owner).to_linear().to_f32_array();
    let mut out = mesh.clone();
    out.colors.iter_mut().zip(tribe).filter(|(_, t)| **t).for_each(|(c, _)| *c = colour);
    out
}

/// Decode just our bundled contract, not an arbitrary scene loader.
fn decode(bytes: &[u8]) -> Option<Decoded> {
    let glb = gltf::Gltf::from_slice(bytes).ok()?;
    let blob = glb.blob.as_deref()?;
    let read = |name| -> Option<(MeshData, Vec<bool>)> {
        let mesh = glb.meshes().find(|m| m.name() == Some(name))?;
        let mut out = MeshData::default();
        let mut tribe = Vec::new();
        for primitive in mesh.primitives() {
            if primitive.mode() != gltf::mesh::Mode::Triangles {
                return None;
            }
            let reader = primitive.reader(|buffer| match buffer.source() {
                gltf::buffer::Source::Bin => Some(blob),
                _ => None,
            });
            let positions: Vec<_> = reader.read_positions()?.collect();
            let normals: Vec<_> = reader.read_normals()?.collect();
            let uvs: Vec<_> = reader.read_tex_coords(0)?.into_f32().collect();
            if positions.len() != normals.len() || positions.len() != uvs.len() {
                return None;
            }
            let material = primitive.material();
            let is_tribe = material.name() == Some("Tribe");
            let colour = material.pbr_metallic_roughness().base_color_factor();
            let indices: Vec<_> = reader.read_indices().map(|i| i.into_u32().collect()).unwrap_or_else(|| (0..positions.len() as u32).collect());
            if indices.len() % 3 != 0 {
                return None;
            }
            for i in indices {
                out.indices.push(out.positions.len() as u32);
                out.positions.push(*positions.get(i as usize)?);
                out.normals.push(*normals.get(i as usize)?);
                out.uvs.push(*uvs.get(i as usize)?);
                out.colors.push(colour);
                tribe.push(is_tribe);
            }
        }
        (!out.indices.is_empty()).then_some((out, tribe))
    };
    let (body, body_tribe) = read("Body")?;
    let (scaffold, scaffold_tribe) = read("Scaffold")?;
    Some(Decoded { model: Model { body, scaffold }, tribe: [body_tribe, scaffold_tribe] })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::construction::built_part;
    use bevy::math::Vec3;

    fn gray(values: &[u8]) -> Vec<u8> {
        values.iter().flat_map(|&v| [v, v, v, 255]).collect()
    }

    #[test]
    fn half_gray_keeps_flat_areas_and_averages_in_linear_light() {
        assert_eq!(half_gray(&gray(&[200; 16]), 4), gray(&[200; 4]));
        // Black and white average to linear mid-grey, sRGB 188, not the sRGB midpoint 128.
        assert_eq!(half_gray(&gray(&[0, 255, 0, 255]), 2), gray(&[188]));
        assert_eq!(half_gray(&gray(&[0, 0, 255, 255, 0, 0, 255, 255, 9, 9, 9, 9, 9, 9, 9, 9]), 4), gray(&[0, 255, 9, 9]));
    }

    #[test]
    fn kit_covers_named_kinds_with_valid_geometry_and_footprints() {
        for id in 1..=19 {
            let kind = BuildingKind::from_model(id);
            assert_eq!(index(kind).is_none(), matches!(kind, BuildingKind::Other(_)), "{kind:?}");
        }
        assert_eq!(index(BuildingKind::Hut { size: 7 }), index(BuildingKind::Hut { size: 3 }));
        for kind in KINDS {
            let model = Model { body: body(kind, 0).expect("named building must have a valid GLB"), scaffold: scaffold(kind, 0).unwrap() };
            for mesh in [&model.body, &model.scaffold] {
                assert_eq!(mesh.positions.len(), mesh.normals.len());
                assert_eq!(mesh.positions.len(), mesh.colors.len());
                assert_eq!(mesh.positions.len(), mesh.uvs.len());
                assert!(mesh.positions.iter().flatten().all(|v| v.is_finite()));
                assert!(mesh.uvs.iter().flatten().all(|v| v.is_finite() && (0.0..=1.0).contains(v)));
                for tri in mesh.indices.chunks_exact(3) {
                    let [a, b, c] = [0, 1, 2].map(|k| Vec3::from(mesh.positions[tri[k] as usize]));
                    let normal = Vec3::from(mesh.normals[tri[0] as usize]);
                    assert!((normal.length() - 1.0).abs() < 1e-5);
                    assert!((b - a).cross(c - a).dot(normal) > 1e-8, "{kind:?}: degenerate or reversed triangle");
                }
            }
            let f = kind.footprint();
            for &[x, y, z] in &model.body.positions {
                // Eaves/posts get 0.12 cell of visual allowance. Boat piers intentionally
                // extend beyond the land footprint on +Z into the launch channel.
                assert!(y >= -0.04 && y < 3.0, "{kind:?}: vertical bounds {y}");
                assert!((x - f.offset.0 as f32 / 512.0).abs() <= f.half.0 as f32 / 512.0 + 0.12, "{kind:?}: x {x}");
                if kind == BuildingKind::BoatHut && z > 0.7 {
                    assert!(z <= 2.1);
                } else {
                    assert!((z - f.offset.1 as f32 / 512.0).abs() <= f.half.1 as f32 / 512.0 + 0.12, "{kind:?}: z {z}");
                }
            }
        }
    }

    #[test]
    fn construction_preserves_colours_and_grows_for_every_wood_piece() {
        for kind in game_core::build_book::BUILDABLE {
            let full = body(kind, 0).unwrap();
            let cost = kind.wood_cost();
            assert!(built_part(&full, 0, cost).indices.is_empty());
            let mut last = 0;
            for used in 1..=cost {
                let part = built_part(&full, used, cost);
                assert!(part.indices.len() > last);
                assert_eq!(part.colors.len(), part.positions.len());
                for ((p, c), uv) in part.positions.iter().zip(&part.colors).zip(&part.uvs) {
                    assert!(full.positions.iter().zip(&full.colors).zip(&full.uvs).any(|((fp, fc), fuv)| p == fp && c == fc && uv == fuv));
                }
                last = part.indices.len();
            }
            assert_eq!(last, full.indices.len());
        }
    }

    #[test]
    fn only_tribe_material_changes_between_owners() {
        for kind in game_core::build_book::BUILDABLE {
            let blue = body(kind, 0).unwrap();
            for owner in [1, 2, 3, 255] {
                let other = body(kind, owner).unwrap();
                assert_eq!(blue.positions, other.positions);
                assert_eq!(blue.uvs, other.uvs);
                let changed: Vec<_> = blue.colors.iter().zip(&other.colors).filter(|(a, b)| a != b).collect();
                assert!(!changed.is_empty() && changed.len() < blue.colors.len());
                assert!(changed.iter().all(|(a, b)| **a == tribe_color(0).to_linear().to_f32_array() && **b == tribe_color(owner).to_linear().to_f32_array()));
            }
            assert_eq!(body(kind, 0).unwrap(), blue, "painting an owner leaves the cached model untouched");
        }
    }

    #[test]
    fn glbs_link_the_shared_atlas_and_uvs_stay_inside_material_gutters() {
        for bytes in MODELS {
            let glb = gltf::Gltf::from_slice(bytes).unwrap();
            let blob = glb.blob.as_deref().unwrap();
            for mesh in glb.meshes() {
                for primitive in mesh.primitives() {
                    let mat = primitive.material();
                    let tile = mat.index().unwrap();
                    let texture = mat.pbr_metallic_roughness().base_color_texture().unwrap();
                    assert_eq!(texture.tex_coord(), 0);
                    let gltf::image::Source::Uri { uri, mime_type } = texture.texture().source().source() else { panic!("atlas must be linked, not embedded") };
                    assert_eq!((uri, mime_type), ("surfaces.png", Some("image/png")));
                    let reader = primitive.reader(|_| Some(blob));
                    let uvs: Vec<_> = reader.read_tex_coords(0).unwrap().into_f32().collect();
                    for [u, v] in &uvs {
                        let x = u * 512.0 - (tile % 4 * 128) as f32;
                        let y = v * 512.0 - (tile / 4 * 128) as f32;
                        assert!((8.49..=119.51).contains(&x) && (8.49..=119.51).contains(&y), "UV outside its material tile");
                    }
                    for tri in uvs.chunks_exact(3) {
                        let a = bevy::math::Vec2::from(tri[1]) - bevy::math::Vec2::from(tri[0]);
                        let b = bevy::math::Vec2::from(tri[2]) - bevy::math::Vec2::from(tri[0]);
                        assert!((a.x * b.y - a.y * b.x).abs() > 1e-10, "collapsed UV triangle");
                    }
                }
            }
        }
        let image = surface_image();
        assert_eq!(image.texture_descriptor.mip_level_count, 5);
        let data = image.data.unwrap();
        assert_eq!(data.len(), [512, 256, 128, 64, 32].into_iter().map(|n| n * n * 4).sum::<usize>());
        assert!(data.chunks_exact(4).all(|p| p[0] == p[1] && p[1] == p[2] && p[3] == 255), "neutral opaque detail preserves tribe hue");
        // The unused bottom-right tile is plain white at every level.
        let mut start = 0;
        for side in [512, 256, 128, 64, 32] {
            assert_eq!(data[start + ((side - 1) * side + side - 1) * 4], 255);
            start += side * side * 4;
        }
    }
}
