//! Blender-style modifier stack: non-destructive operations evaluated on a
//! free mesh, top to bottom, every time its geometry is needed. The mesh
//! itself stays as modelled, so a modifier can be changed, switched off,
//! reordered or applied later.

use crate::EditMesh;
use glam::{DVec2, DVec3};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Axis {
    X,
    Y,
    Z,
}

impl Axis {
    pub const ALL: [Axis; 3] = [Axis::X, Axis::Y, Axis::Z];

    pub fn unit(self) -> DVec3 {
        match self {
            Axis::X => DVec3::X,
            Axis::Y => DVec3::Y,
            Axis::Z => DVec3::Z,
        }
    }

    pub fn index(self) -> usize {
        match self {
            Axis::X => 0,
            Axis::Y => 1,
            Axis::Z => 2,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Axis::X => "X",
            Axis::Y => "Y",
            Axis::Z => "Z",
        }
    }
}

/// Which corners a bevel rounds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Corners {
    #[default]
    All,
    /// Outside corners only.
    Convex,
    /// Inside corners only.
    Concave,
}

/// Which faces a shell leaves open.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum OpenFaces {
    #[default]
    None,
    Top,
    Bottom,
    Both,
}

fn yes() -> bool {
    true
}

fn one() -> f64 {
    1.0
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ModifierKind {
    /// `count` copies, each `offset` further on (in the mesh's own axes;
    /// `relative`: in multiples of its size), or as many as fit
    /// `fit_length`; each turned `turn` degrees and grown `grow` times
    /// more than the one before.
    Array {
        count: u32,
        offset: DVec3,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        relative: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        fit_length: Option<f64>,
        #[serde(default)]
        turn: f64,
        #[serde(default = "one")]
        grow: f64,
    },
    /// Copies round the upright axis through `center` (MU38).
    RadialArray {
        count: u32,
        angle: f64,
        center: DVec2,
        #[serde(default = "yes")]
        turn_copies: bool,
    },
    /// Copies along a path, in the mesh's own axes (MU38).
    PathArray {
        path: Vec<DVec3>,
        count: u32,
        #[serde(default = "yes")]
        align: bool,
    },
    /// Rounded or cut upright corners of an upright extrusion (MU38).
    Bevel {
        radius: f64,
        segments: u32,
        chamfer: bool,
        corners: Corners,
    },
    /// Hollowed to a thickness, open at the top and/or bottom (MU38).
    Shell { thickness: f64, open: OpenFaces },
    /// Floors: cut at heights with a gap between (MU38).
    Slice { heights: Vec<f64>, gap: f64 },
    /// Facade panels on upright rectangular faces (MU38).
    Panelize {
        width: f64,
        height: f64,
        joint: f64,
        depth: f64,
    },
    /// Fins or louvres on the sides facing N, E, S, W (MU38).
    Louvres {
        sides: [bool; 4],
        upright: bool,
        spacing: f64,
        depth: f64,
        thickness: f64,
        angle: f64,
    },
    /// Plan sizes scaled to `x`, `y` at the top (MU38).
    Taper { x: f64, y: f64 },
    /// Turned `angle` degrees from bottom to top (MU38).
    Twist { angle: f64 },
    /// A mirrored copy across the plane normal to `axis` at `plane` along it.
    /// Vertices on the plane are shared and faces lying in it dropped, so a
    /// half model mirrors into one closed mesh.
    Mirror { axis: Axis, plane: f64 },
    /// Thickness for surfaces: an inner skin `thickness` behind the faces,
    /// joined to them along open edges.
    Solidify { thickness: f64 },
    /// Catmull-Clark smoothing, `levels` times; `simple` splits the faces
    /// without rounding them (a finer mesh to edit or panel).
    Subdivision {
        levels: u32,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        simple: bool,
    },
}

impl ModifierKind {
    pub fn label(&self) -> &'static str {
        match self {
            ModifierKind::Array { .. } => "Array",
            ModifierKind::RadialArray { .. } => "Radial array",
            ModifierKind::PathArray { .. } => "Path array",
            ModifierKind::Mirror { .. } => "Mirror",
            ModifierKind::Panelize { .. } => "Panelize",
            ModifierKind::Louvres { .. } => "Louvres / fins",
            ModifierKind::Bevel { .. } => "Bevel",
            ModifierKind::Shell { .. } => "Shell",
            ModifierKind::Solidify { .. } => "Solidify",
            ModifierKind::Slice { .. } => "Slice by levels",
            ModifierKind::Taper { .. } => "Taper",
            ModifierKind::Twist { .. } => "Twist",
            ModifierKind::Subdivision { .. } => "Smooth",
        }
    }

    /// The icon of its card and menu line.
    pub fn icon(&self) -> &'static str {
        match self {
            ModifierKind::Array { .. } => "mod-array",
            ModifierKind::RadialArray { .. } => "mod-radial",
            ModifierKind::PathArray { .. } => "mod-path",
            ModifierKind::Mirror { .. } => "flip-horizontal-2",
            ModifierKind::Panelize { .. } => "mod-panels",
            ModifierKind::Louvres { .. } => "mod-louvres",
            ModifierKind::Bevel { .. } => "mod-bevel",
            ModifierKind::Shell { .. } => "mod-shell",
            ModifierKind::Solidify { .. } => "mod-solidify",
            ModifierKind::Slice { .. } => "mod-slice",
            ModifierKind::Taper { .. } => "mod-taper",
            ModifierKind::Twist { .. } => "mod-twist",
            ModifierKind::Subdivision { .. } => "mod-smooth",
        }
    }

    /// One line on a folded card: its main settings.
    pub fn summary(&self) -> String {
        let mm = |v: f64| format!("{:.0} mm", v * 1000.0);
        match self {
            ModifierKind::Array {
                count, fit_length, ..
            } => match fit_length {
                Some(l) => format!("fit {}", mm(*l)),
                None => format!("{count} copies"),
            },
            ModifierKind::RadialArray { count, angle, .. } => format!("{count} over {angle:.0}°"),
            ModifierKind::PathArray { count, path, .. } => {
                if path.len() < 2 {
                    "no path yet".into()
                } else {
                    format!("{count} along the path")
                }
            }
            ModifierKind::Mirror { axis, .. } => format!("across {}", axis.label()),
            ModifierKind::Panelize { width, height, .. } => {
                format!("{} × {}", mm(*width), mm(*height))
            }
            ModifierKind::Louvres {
                upright, spacing, ..
            } => {
                format!(
                    "{} every {}",
                    if *upright { "fins" } else { "louvres" },
                    mm(*spacing)
                )
            }
            ModifierKind::Bevel {
                radius, chamfer, ..
            } => format!(
                "{} {}",
                if *chamfer { "chamfer" } else { "fillet" },
                mm(*radius)
            ),
            ModifierKind::Shell { thickness, .. } => mm(*thickness),
            ModifierKind::Solidify { thickness } => mm(*thickness),
            ModifierKind::Slice { heights, .. } => format!("{} cuts", heights.len()),
            ModifierKind::Taper { x, y } => format!("top × {x:.2}, {y:.2}"),
            ModifierKind::Twist { angle } => format!("{angle:.0}° over the height"),
            ModifierKind::Subdivision { levels, simple } => {
                format!("{} × {levels}", if *simple { "split" } else { "smooth" })
            }
        }
    }

    /// The modifiers a user can add, by group, with sensible starting
    /// values and a line on what each is for.
    pub fn menu() -> Vec<(&'static str, ModifierKind, &'static str)> {
        vec![
            (
                "Repeat",
                ModifierKind::Array {
                    count: 3,
                    offset: DVec3::new(1.2, 0.0, 0.0),
                    relative: true,
                    fit_length: None,
                    turn: 0.0,
                    grow: 1.0,
                },
                "bays, posts, rows",
            ),
            (
                "Repeat",
                ModifierKind::RadialArray {
                    count: 8,
                    angle: 360.0,
                    center: DVec2::new(3.0, 0.0),
                    turn_copies: true,
                },
                "round a rotunda, seats round a table",
            ),
            (
                "Repeat",
                ModifierKind::PathArray {
                    path: vec![],
                    count: 10,
                    align: true,
                },
                "balusters, posts along a curve",
            ),
            (
                "Repeat",
                ModifierKind::Mirror {
                    axis: Axis::X,
                    plane: 0.0,
                },
                "symmetry",
            ),
            (
                "Facade",
                ModifierKind::Panelize {
                    width: 1.2,
                    height: 0.6,
                    joint: 0.01,
                    depth: 0.03,
                },
                "cladding panels and joints",
            ),
            (
                "Facade",
                ModifierKind::Louvres {
                    sides: [false, true, true, true],
                    upright: true,
                    spacing: 0.4,
                    depth: 0.25,
                    thickness: 0.04,
                    angle: 0.0,
                },
                "fins and louvres for shade",
            ),
            (
                "Shape",
                ModifierKind::Bevel {
                    radius: 0.3,
                    segments: 6,
                    chamfer: false,
                    corners: Corners::All,
                },
                "round or cut the corners",
            ),
            (
                "Shape",
                ModifierKind::Shell {
                    thickness: 0.2,
                    open: OpenFaces::Top,
                },
                "hollow to a thickness",
            ),
            (
                "Shape",
                ModifierKind::Solidify { thickness: 0.2 },
                "thicken a surface",
            ),
            (
                "Shape",
                ModifierKind::Slice {
                    heights: vec![3.0, 6.0],
                    gap: 0.25,
                },
                "floors out of a mass",
            ),
            (
                "Deform",
                ModifierKind::Taper { x: 0.7, y: 0.7 },
                "narrower going up",
            ),
            (
                "Deform",
                ModifierKind::Twist { angle: 45.0 },
                "a turning tower",
            ),
            (
                "Deform",
                ModifierKind::Subdivision {
                    levels: 1,
                    simple: false,
                },
                "soft, organic forms",
            ),
        ]
    }

    /// The new modifiers a user can add, with sensible starting values.
    pub fn presets() -> Vec<ModifierKind> {
        Self::menu().into_iter().map(|m| m.1).collect()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Modifier {
    pub enabled: bool,
    pub kind: ModifierKind,
}

impl Modifier {
    pub fn new(kind: ModifierKind) -> Self {
        Self {
            enabled: true,
            kind,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        match &self.kind {
            ModifierKind::Array {
                count,
                offset,
                fit_length,
                turn,
                grow,
                ..
            } => {
                if !(1..=200).contains(count) {
                    return Err(format!("array count must be 1 to 200, got {count}"));
                }
                if !offset.is_finite() || !turn.is_finite() || !(*grow > 0.0 && grow.is_finite()) {
                    return Err("array offset, turn and growth must be finite".into());
                }
                if fit_length.is_some_and(|l| !(l > 0.0 && l.is_finite())) {
                    return Err("an array's length to fit must be above zero".into());
                }
            }
            ModifierKind::RadialArray {
                count,
                angle,
                center,
                ..
            } => {
                if !(1..=360).contains(count) || !angle.is_finite() || !center.is_finite() {
                    return Err("a radial array needs 1 to 360 copies and a finite angle".into());
                }
            }
            ModifierKind::PathArray { path, count, .. } => {
                if !(1..=500).contains(count) || path.iter().any(|p| !p.is_finite()) {
                    return Err("a path array needs 1 to 500 copies".into());
                }
            }
            ModifierKind::Bevel {
                radius, segments, ..
            } => {
                if !(*radius >= 0.0 && radius.is_finite()) || !(1..=32).contains(segments) {
                    return Err("a bevel needs a radius and 1 to 32 segments".into());
                }
            }
            ModifierKind::Shell { thickness, .. } => {
                if !(*thickness > 1e-6 && thickness.is_finite()) {
                    return Err("a shell needs a thickness".into());
                }
            }
            ModifierKind::Slice { heights, gap } => {
                if heights.iter().any(|h| !h.is_finite()) || !(*gap >= 0.0 && gap.is_finite()) {
                    return Err("a slice needs finite heights and gap".into());
                }
            }
            ModifierKind::Panelize {
                width,
                height,
                joint,
                depth,
            } => {
                if !(*width > 0.01 && *height > 0.01 && *joint >= 0.0 && *depth >= 0.0) {
                    return Err("panels need a size, a joint and a depth".into());
                }
            }
            ModifierKind::Louvres {
                spacing,
                depth,
                thickness,
                angle,
                ..
            } => {
                if !(*spacing > 0.02 && *depth > 0.0 && *thickness > 0.0 && angle.is_finite()) {
                    return Err("louvres need a spacing, a depth and a thickness".into());
                }
            }
            ModifierKind::Taper { x, y } => {
                if !(*x >= 0.0 && *y >= 0.0 && x.is_finite() && y.is_finite()) {
                    return Err("a taper's factors must be 0 or more".into());
                }
            }
            ModifierKind::Twist { angle } => {
                if !angle.is_finite() {
                    return Err("a twist's angle must be finite".into());
                }
            }
            ModifierKind::Mirror { plane, .. } => {
                if !plane.is_finite() {
                    return Err("mirror plane must be finite".into());
                }
            }
            ModifierKind::Solidify { thickness } => {
                if !thickness.is_finite() || thickness.abs() < 1e-6 {
                    return Err("solidify thickness must not be zero".into());
                }
            }
            ModifierKind::Subdivision { levels, .. } => {
                if *levels > 4 {
                    return Err(format!("subdivision levels must be 0 to 4, got {levels}"));
                }
            }
        }
        Ok(())
    }
}

/// A mesh after its modifiers, with the face of the modelled mesh each face
/// came from (`None` for faces a modifier made up, like solidify's rims).
#[derive(Clone, Debug, Default)]
pub struct Evaluated {
    pub mesh: EditMesh,
    pub source: Vec<Option<usize>>,
}

/// Run the enabled modifiers over `mesh`, top to bottom: remembered for
/// the last shapes asked for, so the 3D view, picking under the pointer
/// and the drawings of one shape evaluate its stack once, not on every
/// mouse move (MU39: hovering a panelled, louvred mass re-ran it each
/// frame).
pub fn evaluate(mesh: &EditMesh, stack: &[Modifier]) -> Evaluated {
    if !stack.iter().any(|m| m.enabled) {
        return evaluate_now(mesh, stack);
    }
    use crate::hash::StableHasher;
    use std::hash::{Hash, Hasher};
    let mut h = StableHasher::new();
    for v in &mesh.vertices {
        v.to_array().map(f64::to_bits).hash(&mut h);
    }
    mesh.faces.hash(&mut h);
    format!("{stack:?}").hash(&mut h);
    let key = h.finish();
    type Cache = Vec<(u64, std::sync::Arc<Evaluated>)>;
    static CACHE: std::sync::Mutex<Cache> = std::sync::Mutex::new(Vec::new());
    if let Some(hit) = CACHE
        .lock()
        .ok()
        .and_then(|c| c.iter().find(|(k, _)| *k == key).map(|(_, e)| e.clone()))
    {
        return (*hit).clone();
    }
    let e = evaluate_now(mesh, stack);
    if let Ok(mut c) = CACHE.lock() {
        if c.len() >= 48 {
            c.remove(0);
        }
        c.push((key, std::sync::Arc::new(e.clone())));
    }
    e
}

fn evaluate_now(mesh: &EditMesh, stack: &[Modifier]) -> Evaluated {
    let mut out = Evaluated {
        mesh: mesh.clone(),
        source: (0..mesh.faces.len()).map(Some).collect(),
    };
    for m in stack.iter().filter(|m| m.enabled && m.validate().is_ok()) {
        out = match &m.kind {
            ModifierKind::Array {
                count,
                offset,
                relative,
                fit_length,
                turn,
                grow,
            } => crate::modgen::array(&out, *count, *offset, *relative, *fit_length, *turn, *grow),
            ModifierKind::RadialArray {
                count,
                angle,
                center,
                turn_copies,
            } => crate::modgen::radial(&out, *count, *angle, *center, *turn_copies),
            ModifierKind::PathArray { path, count, align } => {
                crate::modgen::along_path(&out, path, *count, *align)
            }
            ModifierKind::Bevel {
                radius,
                segments,
                chamfer,
                corners,
            } => crate::modgen::bevel(&out, *radius, *segments, *chamfer, *corners),
            ModifierKind::Shell { thickness, open } => {
                crate::modgen::shell(&out, *thickness, *open)
            }
            ModifierKind::Slice { heights, gap } => crate::modgen::slice(&out, heights, *gap),
            ModifierKind::Panelize {
                width,
                height,
                joint,
                depth,
            } => crate::modgen::panelize(&out, *width, *height, *joint, *depth),
            ModifierKind::Louvres {
                sides,
                upright,
                spacing,
                depth,
                thickness,
                angle,
            } => {
                crate::modgen::louvres(&out, *sides, *upright, *spacing, *depth, *thickness, *angle)
            }
            ModifierKind::Taper { x, y } => crate::modgen::taper(&out, *x, *y),
            ModifierKind::Twist { angle } => crate::modgen::twist(&out, *angle),
            ModifierKind::Mirror { axis, plane } => mirror(&out, *axis, *plane),
            ModifierKind::Solidify { thickness } => solidify(&out, *thickness),
            ModifierKind::Subdivision { levels, simple } => {
                let mut e = out;
                for _ in 0..*levels {
                    // Catmull-Clark turns face f into len(f) quads, in order
                    let source = e
                        .mesh
                        .faces
                        .iter()
                        .zip(&e.source)
                        .flat_map(|(f, s)| std::iter::repeat_n(*s, f.len()))
                        .collect();
                    e = Evaluated {
                        mesh: e.mesh.subdivide_with(!simple),
                        source,
                    };
                }
                e
            }
        };
    }
    out
}

fn mirror(e: &Evaluated, axis: Axis, plane: f64) -> Evaluated {
    const ON: f64 = 1e-5;
    let k = axis.index();
    let on_plane = |v: &DVec3| (v[k] - plane).abs() < ON;
    let mut out = Evaluated {
        mesh: EditMesh {
            vertices: e.mesh.vertices.clone(),
            faces: vec![],
        },
        source: vec![],
    };
    // vertices on the plane are shared by both halves
    let map: Vec<u32> = e
        .mesh
        .vertices
        .iter()
        .enumerate()
        .map(|(i, v)| {
            if on_plane(v) {
                i as u32
            } else {
                let mut m = *v;
                m[k] = 2.0 * plane - m[k];
                out.mesh.vertices.push(m);
                (out.mesh.vertices.len() - 1) as u32
            }
        })
        .collect();
    // faces lying in the mirror plane would end up inside the result
    let kept: Vec<usize> = (0..e.mesh.faces.len())
        .filter(|&f| {
            !e.mesh.faces[f]
                .iter()
                .all(|v| on_plane(&e.mesh.vertices[*v as usize]))
        })
        .collect();
    for &f in &kept {
        out.mesh.faces.push(e.mesh.faces[f].clone());
        out.source.push(e.source[f]);
    }
    for &f in &kept {
        // a reflection turns faces inside out, so reverse each loop
        let face = e.mesh.faces[f].iter().rev().map(|v| map[*v as usize]);
        out.mesh.faces.push(face.collect());
        out.source.push(e.source[f]);
    }
    out
}

pub(crate) fn solidify(e: &Evaluated, thickness: f64) -> Evaluated {
    let mesh = &e.mesh;
    let nv = mesh.vertices.len();
    let normals: Vec<DVec3> = (0..mesh.faces.len()).map(|f| mesh.face_normal(f)).collect();
    let mut vn = vec![DVec3::ZERO; nv];
    let mut v_faces: Vec<Vec<usize>> = vec![vec![]; nv];
    for (fi, f) in mesh.faces.iter().enumerate() {
        for &v in f {
            vn[v as usize] += normals[fi];
            v_faces[v as usize].push(fi);
        }
    }
    let mut out = e.clone();
    // the inner skin: each vertex pushed back far enough that every face it
    // touches ends up `thickness` behind itself (even thickness at corners)
    for v in 0..nv {
        let n = vn[v].normalize_or_zero();
        let fit = v_faces[v]
            .iter()
            .map(|&f| n.dot(normals[f]))
            .fold(1.0f64, f64::min)
            .max(0.25);
        out.mesh
            .vertices
            .push(mesh.vertices[v] - n * (thickness / fit));
    }
    let inner = |v: u32| v + nv as u32;
    for f in &mesh.faces {
        out.mesh
            .faces
            .push(f.iter().rev().map(|&v| inner(v)).collect());
        out.source.push(None);
    }
    // rims along open edges, where a surface has a boundary
    let mut uses: HashMap<(u32, u32), Vec<(u32, u32)>> = HashMap::new();
    for f in &mesh.faces {
        for i in 0..f.len() {
            let (a, b) = (f[i], f[(i + 1) % f.len()]);
            uses.entry((a.min(b), a.max(b))).or_default().push((a, b));
        }
    }
    let mut open: Vec<(u32, u32)> = uses
        .values()
        .filter(|u| u.len() == 1)
        .map(|u| u[0])
        .collect();
    open.sort_unstable();
    for (a, b) in open {
        out.mesh.faces.push(vec![b, a, inner(a), inner(b)]);
        out.source.push(None);
    }
    if thickness < 0.0 {
        // a negative thickness grows outwards: the skins swap roles
        for f in &mut out.mesh.faces {
            f.reverse();
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_modifier_takes_a_reshaped_tall_mass() {
        // a box pulled up tall, one corner dragged off square (MU39 crash)
        let mut m = EditMesh::cuboid(DVec3::new(4.0, 3.0, 3.0));
        let top = (0..m.faces.len())
            .find(|f| m.face_normal(*f).z > 0.9)
            .unwrap();
        m.extrude_face(top, 14.0);
        m.vertices[1] += DVec3::new(0.7, -0.4, 0.0);
        for k in ModifierKind::presets() {
            let e = evaluate(&m, &[Modifier::new(k.clone())]);
            assert_eq!(e.source.len(), e.mesh.faces.len(), "{k:?}");
            for f in &e.mesh.faces {
                assert!(
                    f.iter().all(|v| (*v as usize) < e.mesh.vertices.len()),
                    "{k:?}"
                );
            }
        }
    }

    #[test]
    fn an_array_fits_a_length_in_steps_of_its_own_size() {
        let e = evaluate(
            &cube(),
            &[Modifier::new(ModifierKind::Array {
                count: 2,
                offset: DVec3::new(2.0, 0.0, 0.0),
                relative: true,
                fit_length: Some(9.0),
                turn: 0.0,
                grow: 1.0,
            })],
        );
        // 2 m steps in 9 m: five copies
        assert_eq!(e.mesh.faces.len(), 30);
        assert!((crate::modgen::bounds(&e.mesh).1.x - 8.5).abs() < 1e-9);
    }

    #[test]
    fn old_files_read_their_arrays_and_subdivisions() {
        let a: ModifierKind =
            serde_json::from_str(r#"{"Array":{"count":3,"offset":[1.0,0.0,0.0]}}"#).unwrap();
        assert!(matches!(a, ModifierKind::Array { grow, relative: false, .. } if grow == 1.0));
        let s: ModifierKind = serde_json::from_str(r#"{"Subdivision":{"levels":2}}"#).unwrap();
        assert_eq!(
            s,
            ModifierKind::Subdivision {
                levels: 2,
                simple: false
            }
        );
    }

    #[test]
    fn a_simple_split_keeps_the_shape() {
        let e = evaluate(
            &cube(),
            &[Modifier::new(ModifierKind::Subdivision {
                levels: 1,
                simple: true,
            })],
        );
        assert_eq!(e.mesh.faces.len(), 24);
        assert!((e.mesh.volume() - 1.0).abs() < 1e-9);
    }

    fn cube() -> EditMesh {
        EditMesh::cuboid(DVec3::ONE)
    }

    #[test]
    fn array_repeats_the_mesh() {
        let e = evaluate(
            &cube(),
            &[Modifier::new(ModifierKind::Array {
                count: 4,
                offset: DVec3::new(2.0, 0.0, 0.0),
                relative: false,
                fit_length: None,
                turn: 0.0,
                grow: 1.0,
            })],
        );
        assert_eq!(e.mesh.faces.len(), 24);
        assert!((e.mesh.volume() - 4.0).abs() < 1e-9);
        assert_eq!(e.source[6], Some(0));
        e.mesh.validate().unwrap();
    }

    #[test]
    fn mirror_welds_a_half_model_into_one_closed_mesh() {
        // a box from x = 0 to 1, mirrored at x = 0, is a closed 2 x 1 x 1 box
        let mut half = cube();
        for v in &mut half.vertices {
            v.x += 0.5;
        }
        let e = evaluate(
            &half,
            &[Modifier::new(ModifierKind::Mirror {
                axis: Axis::X,
                plane: 0.0,
            })],
        );
        // the face on the mirror plane is dropped from both halves
        assert_eq!(e.mesh.faces.len(), 10);
        assert_eq!(e.mesh.vertices.len(), 12);
        assert!((e.mesh.volume() - 2.0).abs() < 1e-9);
        e.mesh.validate().unwrap();
    }

    #[test]
    fn solidify_hollows_a_box_with_even_walls() {
        let big = EditMesh::cuboid(DVec3::new(4.0, 4.0, 4.0));
        let e = evaluate(
            &big,
            &[Modifier::new(ModifierKind::Solidify { thickness: 0.5 })],
        );
        // 4³ minus the 3³ hole left by 0.5 m walls all round
        assert!((e.mesh.volume() - (64.0 - 27.0)).abs() < 1e-6);
        assert_eq!(e.mesh.faces.len(), 12);
    }

    #[test]
    fn solidify_gives_a_surface_thickness_with_rims() {
        let sheet = EditMesh {
            vertices: vec![
                DVec3::new(0.0, 0.0, 0.0),
                DVec3::new(2.0, 0.0, 0.0),
                DVec3::new(2.0, 3.0, 0.0),
                DVec3::new(0.0, 3.0, 0.0),
            ],
            faces: vec![vec![0, 1, 2, 3]],
        };
        let e = evaluate(
            &sheet,
            &[Modifier::new(ModifierKind::Solidify { thickness: 0.1 })],
        );
        assert_eq!(e.mesh.faces.len(), 6);
        assert!((e.mesh.volume() - 0.6).abs() < 1e-9);
        assert_eq!(e.source, vec![Some(0), None, None, None, None, None]);
    }

    #[test]
    fn subdivision_tracks_where_faces_came_from_and_disabled_modifiers_do_nothing() {
        let mut stack = vec![Modifier::new(ModifierKind::Subdivision {
            levels: 2,
            simple: false,
        })];
        let e = evaluate(&cube(), &stack);
        assert_eq!(e.mesh.faces.len(), 96);
        assert!(e.source[..16].iter().all(|s| *s == Some(0)));
        stack[0].enabled = false;
        assert_eq!(evaluate(&cube(), &stack).mesh, cube());
    }
}
