//! The site around a building: the ground, hard landscape and planting.
//!
//! - **Terrain** is a ground surface over an outline, `thickness` deep:
//!   flat at its `top`, or following spot heights, between which it is
//!   interpolated (inverse distance weighting). Plans can show its
//!   contours. Hard landscape areas lying in a flat terrain are left out
//!   of it, so paving sits flush in the ground instead of on it.
//! - **Areas** are slab-like layers of hard landscape (paving, gravel, a
//!   driveway, decking), each with a material and a surface pattern that
//!   plans draw over it.
//! - **Plants** are trees and shrubs: a symbol the size of their canopy in
//!   plan, and a simple form (a trunk and a crown) in 3D and elevation.
//!
//! Cars, people and other entourage are catalogue objects (see
//! [`crate::catalogue`]). What each pattern or plant stands for is data:
//! the sizes and spacings are the element's own.

use crate::{styles::LinePattern, styles::Rgb, transform::Xform, ElementId, Model};
use glam::{DVec2, DVec3};
use serde::{Deserialize, Serialize};

/// A site element on a level (usually the ground floor's or a site level).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Site {
    pub level: ElementId,
    pub form: SiteForm,
    /// What it is made of: the ground's, the area's surface, a plant's crown.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub material: Option<ElementId>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum SiteForm {
    /// A ground surface over `boundary` (counter-clockwise), `top` above
    /// the level where it is flat, `thickness` deep below its lowest point.
    Terrain {
        boundary: Vec<DVec2>,
        #[serde(default)]
        top: f64,
        thickness: f64,
        /// Spot heights inside it (`z` above the level); none is flat.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        points: Vec<DVec3>,
        /// Contours every so much height in plan; 0 draws none.
        #[serde(default, skip_serializing_if = "is_zero")]
        contour: f64,
    },
    /// Hard landscape: a layer `thickness` deep with its top `top` above
    /// the level, less its holes, its surface patterned.
    Area {
        boundary: Vec<DVec2>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        holes: Vec<Vec<DVec2>>,
        #[serde(default)]
        top: f64,
        thickness: f64,
        #[serde(default)]
        pattern: Pattern,
    },
    /// A tree or shrub standing at `position`, its base `base` above the
    /// level: `canopy` across its crown, `height` to its top.
    Plant {
        position: DVec2,
        #[serde(default)]
        base: f64,
        form: PlantForm,
        canopy: f64,
        height: f64,
    },
}

fn is_zero(v: &f64) -> bool {
    *v == 0.0
}

/// The pattern drawn over an area's surface in plan.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum Pattern {
    /// Its outline only.
    #[default]
    None,
    /// Rectangular units `size` (along `angle`, across) with joints
    /// between: flags, blocks, setts. `stagger` offsets every other course
    /// by half a unit, as stretcher bond.
    Units {
        size: DVec2,
        #[serde(default)]
        stagger: bool,
        /// Degrees, counter-clockwise from the x axis.
        #[serde(default)]
        angle: f64,
    },
    /// Parallel lines `spacing` apart: boards, decking, ribbed concrete.
    Lines {
        spacing: f64,
        #[serde(default)]
        angle: f64,
    },
    /// Scattered dots about `spacing` apart: gravel, chippings, sand.
    Stipple { spacing: f64 },
}

impl Pattern {
    pub fn label(&self) -> &'static str {
        match self {
            Pattern::None => "None",
            Pattern::Units { .. } => "Units",
            Pattern::Lines { .. } => "Lines",
            Pattern::Stipple { .. } => "Stipple",
        }
    }

    fn validate(&self) -> Result<(), String> {
        let ok = match *self {
            Pattern::None => true,
            Pattern::Units { size, angle, .. } => {
                size.is_finite() && size.min_element() > 1e-3 && angle.is_finite()
            }
            Pattern::Lines { spacing, angle } => {
                spacing.is_finite() && spacing > 1e-3 && angle.is_finite()
            }
            Pattern::Stipple { spacing } => spacing.is_finite() && spacing > 1e-3,
        };
        if ok {
            Ok(())
        } else {
            Err("a surface pattern's sizes must be positive".into())
        }
    }
}

/// The shape a plant is drawn as.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlantForm {
    /// A broadleaf tree: a clear trunk under a round crown.
    #[default]
    Tree,
    /// A conifer: a cone from near the ground.
    Conifer,
    /// A palm: a tall bare trunk and a head of fronds.
    Palm,
    /// A shrub or bush: a low mound from the ground.
    Shrub,
}

impl PlantForm {
    pub const ALL: [PlantForm; 4] = [
        PlantForm::Tree,
        PlantForm::Conifer,
        PlantForm::Palm,
        PlantForm::Shrub,
    ];

    pub fn label(self) -> &'static str {
        match self {
            PlantForm::Tree => "Tree",
            PlantForm::Conifer => "Conifer",
            PlantForm::Palm => "Palm",
            PlantForm::Shrub => "Shrub",
        }
    }
}

impl Site {
    /// A flat terrain over `boundary`.
    pub fn terrain(level: ElementId, mut boundary: Vec<DVec2>, top: f64, thickness: f64) -> Self {
        ccw(&mut boundary);
        Self {
            level,
            form: SiteForm::Terrain {
                boundary,
                top,
                thickness,
                points: vec![],
                contour: 0.0,
            },
            material: None,
        }
    }

    /// A hard landscape area over `boundary`.
    pub fn area(
        level: ElementId,
        mut boundary: Vec<DVec2>,
        thickness: f64,
        pattern: Pattern,
    ) -> Self {
        ccw(&mut boundary);
        Self {
            level,
            form: SiteForm::Area {
                boundary,
                holes: vec![],
                top: 0.0,
                thickness,
                pattern,
            },
            material: None,
        }
    }

    /// A plant at `position`.
    pub fn plant(
        level: ElementId,
        position: DVec2,
        form: PlantForm,
        canopy: f64,
        height: f64,
    ) -> Self {
        Self {
            level,
            form: SiteForm::Plant {
                position,
                base: 0.0,
                form,
                canopy,
                height,
            },
            material: None,
        }
    }

    /// "Terrain", "Hard landscape", "Tree"...: what it is called by default.
    pub fn label(&self) -> &'static str {
        match &self.form {
            SiteForm::Terrain { .. } => "Terrain",
            SiteForm::Area { .. } => "Hard landscape",
            SiteForm::Plant { form, .. } => form.label(),
        }
    }

    pub fn is_plant(&self) -> bool {
        matches!(self.form, SiteForm::Plant { .. })
    }

    /// Ground and areas can be cut by voids (a pool, a light well);
    /// plants can't.
    pub fn cuttable(&self) -> bool {
        !self.is_plant()
    }

    /// The outline of a terrain or an area.
    pub fn boundary(&self) -> Option<&[DVec2]> {
        match &self.form {
            SiteForm::Terrain { boundary, .. } | SiteForm::Area { boundary, .. } => Some(boundary),
            SiteForm::Plant { .. } => None,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        let positive = |v: f64, what: &str| {
            if v.is_finite() && v > 0.0 {
                Ok(())
            } else {
                Err(format!("{what} must be positive, got {v}"))
            }
        };
        let outline = |b: &[DVec2]| {
            if b.len() >= 3 && b.iter().all(|p| p.is_finite()) {
                Ok(())
            } else {
                Err("a site outline needs three finite corners".to_string())
            }
        };
        match &self.form {
            SiteForm::Terrain {
                boundary,
                top,
                thickness,
                points,
                contour,
            } => {
                outline(boundary)?;
                positive(*thickness, "terrain thickness")?;
                if !top.is_finite() || points.iter().any(|p| !p.is_finite()) {
                    return Err("terrain heights must be finite".into());
                }
                if !(contour.is_finite() && *contour >= 0.0) {
                    return Err("the contour interval can't be negative".into());
                }
                Ok(())
            }
            SiteForm::Area {
                boundary,
                holes,
                top,
                thickness,
                pattern,
            } => {
                outline(boundary)?;
                holes.iter().try_for_each(|h| outline(h))?;
                positive(*thickness, "area thickness")?;
                if !top.is_finite() {
                    return Err("an area's top must be finite".into());
                }
                pattern.validate()
            }
            SiteForm::Plant {
                position,
                base,
                canopy,
                height,
                ..
            } => {
                if !(position.is_finite() && base.is_finite()) {
                    return Err("a plant's position must be finite".into());
                }
                positive(*canopy, "canopy size")?;
                positive(*height, "plant height")
            }
        }
    }

    /// The height of a terrain's surface above its level at `p`: its top
    /// when flat, else weighted towards the nearest spot heights.
    pub fn ground_at(&self, p: DVec2) -> f64 {
        match &self.form {
            SiteForm::Terrain {
                boundary,
                top,
                points,
                ..
            } => {
                if points.is_empty() {
                    return *top;
                }
                // the outline's corners stay at the terrain's top unless a
                // spot height is on them
                let corners = boundary
                    .iter()
                    .filter(|c| !points.iter().any(|q| q.truncate().distance(**c) < 1e-3))
                    .map(|c| c.extend(*top));
                let controls: Vec<DVec3> = points.iter().copied().chain(corners).collect();
                surface_height(*top, &controls, p)
            }
            SiteForm::Area { top, .. } => *top,
            SiteForm::Plant { base, .. } => *base,
        }
    }

    /// The corners of its bounding box, in model coordinates.
    pub fn bounds(&self, model: &Model) -> Option<(DVec3, DVec3)> {
        let z = crate::members::elevation(model, self.level);
        match &self.form {
            SiteForm::Terrain {
                boundary,
                top,
                thickness,
                points,
                ..
            } => {
                let (lo, hi) = flat_bounds(boundary)?;
                let zs = points.iter().map(|p| p.z).chain([*top]);
                let (zlo, zhi) = zs.fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), v| {
                    (a.min(v), b.max(v))
                });
                Some((lo.extend(z + zlo - thickness), hi.extend(z + zhi)))
            }
            SiteForm::Area {
                boundary,
                top,
                thickness,
                ..
            } => {
                let (lo, hi) = flat_bounds(boundary)?;
                Some((lo.extend(z + top - thickness), hi.extend(z + top)))
            }
            SiteForm::Plant {
                position,
                base,
                canopy,
                height,
                ..
            } => {
                let r = DVec2::splat(canopy / 2.0);
                Some((
                    (*position - r).extend(z + base),
                    (*position + r).extend(z + base + height),
                ))
            }
        }
    }

    /// Where it is, for turning and scaling it about itself.
    pub fn pivot(&self, model: &Model) -> Option<DVec3> {
        let (lo, hi) = self.bounds(model)?;
        Some(((lo + hi) / 2.0).truncate().extend(lo.z))
    }
}

fn ccw(pts: &mut [DVec2]) {
    if crate::polygon::signed_area2(pts) < 0.0 {
        pts.reverse();
    }
}

fn flat_bounds(pts: &[DVec2]) -> Option<(DVec2, DVec2)> {
    let first = *pts.first()?;
    Some(
        pts.iter()
            .fold((first, first), |(lo, hi), p| (lo.min(*p), hi.max(*p))),
    )
}

/// A surface through spot heights, inverse distance weighted (power 2);
/// `flat` where there are none.
pub fn surface_height(flat: f64, points: &[DVec3], p: DVec2) -> f64 {
    if points.is_empty() {
        return flat;
    }
    let (mut sum, mut weight) = (0.0, 0.0);
    for q in points {
        let d2 = q.truncate().distance_squared(p);
        if d2 < 1e-12 {
            return q.z;
        }
        let w = 1.0 / d2;
        sum += w * q.z;
        weight += w;
    }
    sum / weight
}

/// Is `inner` inside `outer` and clear of its edges (a hole touching
/// the outline would split the ground)?
fn clear_inside(inner: &[DVec2], outer: &[DVec2]) -> bool {
    let near = |p: DVec2, poly: &[DVec2]| {
        (0..poly.len()).any(|i| {
            let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
            let d = b - a;
            let t = ((p - a).dot(d) / d.length_squared().max(1e-12)).clamp(0.0, 1.0);
            p.distance(a + d * t) < 1e-3
        })
    };
    inner
        .iter()
        .all(|p| crate::polygon::contains(outer, *p) && !near(*p, outer))
        && outer.iter().all(|p| !near(*p, inner))
}

/// The hard landscape areas a flat terrain leaves room for: those on its
/// level lying inside it, whose top is in its depth. Their outlines, as
/// holes in the ground.
pub fn ground_holes(model: &Model, terrain: &Site) -> Vec<Vec<DVec2>> {
    let SiteForm::Terrain {
        boundary,
        top,
        thickness,
        points,
        ..
    } = &terrain.form
    else {
        return vec![];
    };
    if !points.is_empty() {
        return vec![];
    }
    model
        .iter()
        .filter_map(|e| match &e.kind {
            crate::ElementKind::Site(Site {
                level,
                form:
                    SiteForm::Area {
                        boundary: b,
                        top: t,
                        ..
                    },
                ..
            }) if *level == terrain.level
                && *t <= top + 0.1
                && *t > top - thickness
                && clear_inside(b, boundary) =>
            {
                Some(b.clone())
            }
            _ => None,
        })
        .collect()
}

/// Move, turn or scale a site element; it stays upright.
pub fn transform(site: &mut Site, t: &Xform) -> Option<()> {
    let dz = match t {
        Xform::Move(d) => d.z,
        _ => 0.0,
    };
    let factor = match t {
        Xform::Scale { factor, .. } => Some(*factor),
        Xform::Stretch { .. } if site.is_plant() => return None,
        _ => None,
    };
    match &mut site.form {
        SiteForm::Terrain {
            boundary,
            top,
            points,
            ..
        } => {
            for p in boundary.iter_mut() {
                *p = t.point2(*p);
            }
            ccw(boundary);
            for p in points.iter_mut() {
                *p = t.point2(p.truncate()).extend(p.z + dz);
            }
            *top += dz;
        }
        SiteForm::Area {
            boundary,
            holes,
            top,
            ..
        } => {
            for p in boundary.iter_mut().chain(holes.iter_mut().flatten()) {
                *p = t.point2(*p);
            }
            ccw(boundary);
            *top += dz;
        }
        SiteForm::Plant {
            position,
            base,
            canopy,
            height,
            ..
        } => {
            *position = t.point2(*position);
            *base += dz;
            if let Some(f) = factor {
                *canopy *= f;
                *height *= f;
            }
        }
    }
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Element, ElementKind, Level};

    #[test]
    fn spot_heights_shape_the_ground() {
        let pts = [DVec3::new(0.0, 0.0, 0.0), DVec3::new(10.0, 0.0, 2.0)];
        assert_eq!(surface_height(0.5, &[], DVec2::ZERO), 0.5);
        assert_eq!(surface_height(0.5, &pts, DVec2::new(10.0, 0.0)), 2.0);
        let mid = surface_height(0.5, &pts, DVec2::new(5.0, 0.0));
        assert!((mid - 1.0).abs() < 1e-9);
    }

    #[test]
    fn paving_sits_in_the_ground() {
        let lv = Element::new(
            "Site",
            ElementKind::Level(Level {
                elevation: 0.0,
                height: 3.0,
            }),
        );
        let l = lv.id;
        let ground = Site::terrain(
            l,
            crate::polygon::rectangle(DVec2::splat(-20.0), DVec2::splat(20.0)),
            0.0,
            0.5,
        );
        let drive = Site::area(
            l,
            crate::polygon::rectangle(DVec2::ZERO, DVec2::new(3.0, 8.0)),
            0.15,
            Pattern::Units {
                size: DVec2::new(0.2, 0.1),
                stagger: true,
                angle: 0.0,
            },
        );
        let outside = Site::area(
            l,
            crate::polygon::rectangle(DVec2::splat(30.0), DVec2::splat(32.0)),
            0.15,
            Pattern::None,
        );
        let m = Model::from_elements([
            lv,
            Element::new("Ground", ElementKind::Site(ground.clone())),
            Element::new("Drive", ElementKind::Site(drive)),
            Element::new("Far", ElementKind::Site(outside)),
        ])
        .unwrap();
        assert_eq!(ground_holes(&m, &ground).len(), 1);
    }

    #[test]
    fn plants_scale_but_terrain_stretches() {
        let mut p = Site::plant(ElementId::new(), DVec2::ZERO, PlantForm::Tree, 4.0, 6.0);
        let stretch = Xform::Stretch {
            center: DVec3::ZERO,
            factor: DVec3::new(2.0, 1.0, 1.0),
        };
        assert!(transform(&mut p, &stretch).is_none());
        transform(
            &mut p,
            &Xform::Scale {
                center: DVec3::ZERO,
                factor: 0.5,
            },
        )
        .unwrap();
        assert!(matches!(p.form, SiteForm::Plant { canopy, .. } if canopy == 2.0));
        let mut t = Site::terrain(
            ElementId::new(),
            crate::polygon::rectangle(DVec2::ZERO, DVec2::ONE),
            0.0,
            0.3,
        );
        assert!(transform(&mut t, &stretch).is_some());
        assert!(t.validate().is_ok());
    }
}

/// A loaded map layer: 2D vector geography from a GeoPackage — BGT
/// parcels/roads/water/green, BAG footprints, cadastral parcels.
///
/// One element per source layer; features inside are keyed by source
/// row-id, so a giskit upgrade syncs surgically (small journal,
/// manual edits elsewhere survive). Never meshed: mesh conversion
/// futzes the drawing (fan edges instead of the true outline,
/// categories reduced to name strings). The plan draws the true
/// outlines with the category's hatch; selection and provenance ride
/// the element like any other.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum SiteCategory {
    Parcel,
    Building,
    Road,
    Water,
    Green,
    Structure,
    Boundary,
    /// Layers no variant covers yet.
    #[serde(untagged)]
    Other(String),
}

impl SiteCategory {
    /// The plan hatch for the category.
    pub fn hatch(&self) -> crate::Hatch {
        use crate::Hatch::*;
        match self {
            SiteCategory::Parcel => Empty,
            SiteCategory::Building => Solid,
            SiteCategory::Road => Diagonal,
            SiteCategory::Water => Cross,
            SiteCategory::Green => Timber,
            SiteCategory::Structure => Concrete,
            SiteCategory::Boundary => Empty,
            // Unmapped layers draw as bare outlines: quiet, and visibly
            // uncategorised (recategorise the element to fill them).
            SiteCategory::Other(_) => Empty,
        }
    }

    /// The BGT/kadaster layer names that map onto each category.
    /// Recipe defaults; projects override by editing the element.
    /// Matches the last `_`-separated segment, so provider prefixes
    /// (`bgt_`, `kadastralekaart-wfs_`, …) fall away.
    pub fn of_layer(layer: &str) -> Self {
        // BAG3D detail layers are buildings whatever the LoD suffix.
        if layer.starts_with("bag3d") {
            return SiteCategory::Building;
        }
        let base = layer.rsplit('/').next().unwrap_or(layer);
        // BGT geometry suffixes are not part of the name.
        let base = base
            .strip_suffix("_vlak")
            .or_else(|| base.strip_suffix("_lijn"))
            .or_else(|| base.strip_suffix("_punt"))
            .unwrap_or(base);
        let base = base.rsplit('_').next().unwrap_or(base);
        match base {
            "perceel" => SiteCategory::Parcel,
            "pand" | "bebouwing" => SiteCategory::Building,
            "wegdeel" | "ondersteunendwegdeel" | "openbareruimte" => SiteCategory::Road,
            "waterdeel" | "ondersteunendwaterdeel" => SiteCategory::Water,
            "begroeidterreindeel" | "vegetatieobject" => SiteCategory::Green,
            "overbruggingsdeel" | "tunneldeel" | "overigbouwwerk" | "kunstwerkdeel" => {
                SiteCategory::Structure
            }
            "kadastrale-grens" | "grens" | "scheiding" => SiteCategory::Boundary,
            other => SiteCategory::Other(other.to_string()),
        }
    }
}

/// One feature's shape: a filled polygon (with holes) or an open polyline.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum SiteGeometry {
    Polygon {
        exterior: Vec<DVec2>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        holes: Vec<Vec<DVec2>>,
    },
    Polyline {
        points: Vec<DVec2>,
    },
}

impl SiteGeometry {
    fn validate(&self) -> Result<(), String> {
        let finite = |p: &DVec2| p.is_finite();
        match self {
            SiteGeometry::Polygon { exterior, holes } => {
                if exterior.len() < 3 || !exterior.iter().all(finite) {
                    return Err("a site polygon needs 3 finite corners".into());
                }
                for h in holes {
                    if h.len() < 3 || !h.iter().all(finite) {
                        return Err("a site hole needs 3 finite corners".into());
                    }
                }
                Ok(())
            }
            SiteGeometry::Polyline { points } => {
                if points.len() < 2 || !points.iter().all(finite) {
                    return Err("a site polyline needs 2 finite points".into());
                }
                Ok(())
            }
        }
    }

    /// Every point, for transforms.
    pub(crate) fn map(&mut self, f: impl Fn(DVec2) -> DVec2) {
        match self {
            SiteGeometry::Polygon { exterior, holes } => {
                for p in exterior.iter_mut() {
                    *p = f(*p);
                }
                for h in holes {
                    for p in h.iter_mut() {
                        *p = f(*p);
                    }
                }
            }
            SiteGeometry::Polyline { points } => {
                for p in points.iter_mut() {
                    *p = f(*p);
                }
            }
        }
    }
}

/// One row of the source layer, keyed by its id there.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SiteRow {
    pub key: String,
    pub geom: SiteGeometry,
}

/// One source layer's worth of site geography.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SiteFeature {
    /// The source layer name (what the registry and sync match on).
    pub layer: String,
    pub category: SiteCategory,
    pub features: Vec<SiteRow>,
    /// How the layer draws, project-wide, as its editor sets it. A
    /// reload rewrites the features and the category but keeps this:
    /// the layer's look is the project's, not the file's.
    #[serde(default)]
    pub style: LayerStyle,
}

/// How one linked layer draws: its fill and line colours, its line
/// pattern, whether it shows at all. What is left out draws as the
/// category does. (Hiding single rows of a layer — a neighbour's
/// extension you don't want — is a `hidden_rows` list here later.)
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LayerStyle {
    /// Under the category hatch; none keeps the hatch's own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill: Option<Rgb>,
    /// The outlines and polylines; none keeps the pen's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<Rgb>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern: Option<LinePattern>,
    #[serde(default = "default_visible")]
    pub visible: bool,
}

fn default_visible() -> bool {
    true
}

impl Default for LayerStyle {
    fn default() -> Self {
        Self {
            fill: None,
            line: None,
            pattern: None,
            visible: true,
        }
    }
}

impl LayerStyle {
    /// Nothing set: the category draws as it always did.
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

impl SiteFeature {
    pub fn validate(&self) -> Result<(), String> {
        if self.layer.is_empty() {
            return Err("a site layer needs a name".into());
        }
        let mut seen = std::collections::HashSet::new();
        for f in &self.features {
            if !seen.insert(&f.key) {
                return Err(format!("site row {:?} is listed twice", f.key));
            }
            f.geom.validate()?;
        }
        Ok(())
    }

    /// The plan area, in square metres (polygons only).
    pub fn area(&self) -> f64 {
        self.features
            .iter()
            .filter_map(|f| match &f.geom {
                SiteGeometry::Polygon { exterior, holes } => {
                    let ring = |pts: &[DVec2]| {
                        pts.iter()
                            .zip(pts.iter().skip(1).chain(pts.first()))
                            .map(|(a, b)| a.x * b.y - b.x * a.y)
                            .sum::<f64>()
                            .abs()
                            / 2.0
                    };
                    Some(ring(exterior) - holes.iter().map(|h| ring(h)).sum::<f64>())
                }
                _ => None,
            })
            .sum()
    }
}

#[cfg(test)]
mod site_feature_tests {
    use super::*;

    fn poly(n: usize) -> SiteGeometry {
        SiteGeometry::Polygon {
            exterior: (0..n)
                .map(|i| DVec2::new(i as f64, (i % 2) as f64))
                .collect(),
            holes: vec![],
        }
    }

    #[test]
    fn validation_catches_degenerate_shapes() {
        assert!(poly(3).validate().is_ok());
        assert!(poly(2).validate().is_err());
        assert!(SiteGeometry::Polyline {
            points: vec![DVec2::new(f64::NAN, 0.0), DVec2::X],
        }
        .validate()
        .is_err());
        let mut s = SiteFeature {
            layer: "".into(),
            category: SiteCategory::Parcel,
            features: vec![],
            style: LayerStyle::default(),
        };
        assert!(s.validate().is_err());
        s.layer = "perceel".into();
        s.features = vec![
            SiteRow {
                key: "1".into(),
                geom: poly(4),
            },
            SiteRow {
                key: "1".into(),
                geom: poly(4),
            },
        ];
        assert!(s.validate().is_err());
    }

    #[test]
    fn area_counts_polygons_minus_holes() {
        let s = SiteFeature {
            layer: "x".into(),
            category: SiteCategory::Parcel,
            style: LayerStyle::default(),
            features: vec![SiteRow {
                key: "1".into(),
                geom: SiteGeometry::Polygon {
                    exterior: vec![
                        DVec2::new(0.0, 0.0),
                        DVec2::new(4.0, 0.0),
                        DVec2::new(4.0, 3.0),
                        DVec2::new(0.0, 3.0),
                    ],
                    holes: vec![vec![
                        DVec2::new(1.0, 1.0),
                        DVec2::new(2.0, 1.0),
                        DVec2::new(2.0, 2.0),
                        DVec2::new(1.0, 2.0),
                    ]],
                },
            }],
        };
        assert!((s.area() - 11.0).abs() < 1e-9);
    }

    #[test]
    fn layer_names_map_to_categories() {
        assert_eq!(SiteCategory::of_layer("bgt_wegdeel"), SiteCategory::Road);
        assert_eq!(
            SiteCategory::of_layer("kadastralekaart-wfs_perceel"),
            SiteCategory::Parcel
        );
        assert_eq!(SiteCategory::of_layer("bag_pand"), SiteCategory::Building);
        assert_eq!(SiteCategory::of_layer("bgt_waterdeel"), SiteCategory::Water);
        assert!(matches!(
            SiteCategory::of_layer("something_new"),
            SiteCategory::Other(_)
        ));
    }

    fn linked_layer() -> (crate::Document, crate::ElementId) {
        use crate::{Document, Element, ElementKind, Model, Source, Transaction};
        let mut doc = Document::from_model(Model::new());
        let mut el = Element::new(
            "Site bgt_wegdeel",
            ElementKind::SiteFeature(SiteFeature {
                layer: "bgt_wegdeel".into(),
                category: SiteCategory::Road,
                style: LayerStyle::default(),
                features: vec![SiteRow {
                    key: "1".into(),
                    geom: poly(4),
                }],
            }),
        );
        el.source = Some(Source {
            file: "site.gpkg".into(),
            object: "bgt_wegdeel".into(),
            stamp: 1,
        });
        let id = el.id;
        doc.commit(Transaction::new("link").insert(el)).unwrap();
        (doc, id)
    }

    #[test]
    fn linked_layers_reject_casual_replaces() {
        use crate::{ModelError, Transaction};
        let (mut doc, id) = linked_layer();
        // A hand edit fails loudly, naming the source file.
        let err = doc
            .edit(id, "Move layer", |e| e.name = "moved".into())
            .unwrap_err();
        assert!(
            matches!(err, ModelError::ManagedBySite { .. }),
            "direct replace is locked: {err:?}"
        );
        // The site editor's own transactions pass (labelled `site …`).
        let before = doc.model().get(id).cloned().unwrap();
        let mut after = before.clone();
        after.name = "moved by editor".into();
        doc.commit(Transaction::new("site move site.gpkg").replace(before, after))
            .expect("editor rewrites pass");
        // Removes stay allowed: reload re-creates known layers.
        let gone = doc.model().get(id).cloned().unwrap();
        doc.commit(Transaction::new("unlink").remove_cascade(doc.model(), gone.id))
            .expect("removes pass");
        assert!(doc.model().get(id).is_none());
    }

    #[test]
    fn a_style_survives_and_old_files_load() {
        // a layer saved before LayerStyle existed: no `style` in the JSON
        let old = r#"{"layer":"wijk","category":"Road","features":[{"key":"1",
            "geom":{"Polygon":{"exterior":[[0,0],[10,0],[10,10]]}}}]}"#;
        let s: SiteFeature = serde_json::from_str(old).unwrap();
        assert_eq!(s.style, LayerStyle::default());
        let round: SiteFeature =
            serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(round, s);
        // a styled one keeps its style through the round trip
        let styled = SiteFeature {
            layer: "wijk".into(),
            category: SiteCategory::Road,
            style: LayerStyle {
                fill: Some(Rgb([0.2, 0.4, 0.6])),
                line: None,
                pattern: Some(LinePattern::Dashed),
                visible: false,
            },
            features: s.features,
        };
        let back: SiteFeature =
            serde_json::from_str(&serde_json::to_string(&styled).unwrap()).unwrap();
        assert_eq!(back, styled);
        assert!(!back.style.is_default());
    }
}
