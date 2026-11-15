//! The project's own information: what it is, where, and for whom. One
//! element at most holds it; every field is optional. It fills the
//! sheets' title blocks (under any value a sheet sets itself) and names
//! the project, site and building in IFC exports (A7-5).

use glam::DVec2;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ProjectInfo {
    /// The project's name ("Maple Lane House").
    pub name: String,
    /// The office's job number.
    pub number: String,
    pub client: String,
    /// "Planning application", "Tender", "Construction".
    pub status: String,
    /// The issue date, as the office writes it.
    pub issue_date: String,
    /// The site's postal address.
    pub address: String,
    pub site_name: String,
    /// Degrees, north positive.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latitude: Option<f64>,
    /// Degrees, east positive.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub longitude: Option<f64>,
    /// The project's zero above sea level, metres.
    pub elevation: f64,
    /// How far true north turns from the drawings' up, degrees anticlockwise.
    pub true_north: f64,
    /// The CRS the easting/northing below are in, e.g. `"EPSG:28992"`
    /// (RD New). Absent: the project is not placed on the map.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub srid: Option<String>,
    /// Easting of the model origin (0,0) in `srid` metres.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub easting: Option<f64>,
    /// Northing of the model origin (0,0) in `srid` metres.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub northing: Option<f64>,
    pub building_name: String,
    /// "Dwelling", "Office", "School".
    pub building_use: String,
    /// The project's notepad (MU26): notes and to-dos, markdown, kept with
    /// the project and never printed. Read into [`ProjectInfo::pad`] once.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub notes: String,
    /// The notepad's items (MU27): to-dos and notes, each with its text
    /// (the first line its title) and, if wanted, a link to an element or
    /// a view.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub pad: Vec<PadItem>,
    /// The project's own profiles (MU27 block 5), for sweeps.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub profiles: Vec<crate::profiles::NamedProfile>,
}

/// One item of the project's notepad.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PadItem {
    pub text: String,
    /// A to-do (ticked or not); else a note.
    pub todo: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub done: bool,
    /// What it is about: an element or a view, selected or opened from it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link: Option<crate::ElementId>,
}

impl PadItem {
    /// Its first line.
    pub fn title(&self) -> &str {
        self.text.lines().next().unwrap_or("").trim()
    }
}

/// The old markdown notepad as items: `- [ ]` and `- [x]` lines are
/// to-dos, other lines join into notes, a blank line or heading ending one.
pub fn pad_from_markdown(md: &str) -> Vec<PadItem> {
    let mut out: Vec<PadItem> = vec![];
    let mut note: Option<PadItem> = None;
    let flush = |note: &mut Option<PadItem>, out: &mut Vec<PadItem>| {
        if let Some(n) = note.take().filter(|n| !n.text.trim().is_empty()) {
            out.push(n);
        }
    };
    for line in md.lines() {
        let t = line.trim();
        let todo = t.strip_prefix("- [ ] ").map(|r| (false, r)).or_else(|| {
            t.strip_prefix("- [x] ")
                .or(t.strip_prefix("- [X] "))
                .map(|r| (true, r))
        });
        if let Some((done, r)) = todo {
            flush(&mut note, &mut out);
            out.push(PadItem {
                text: r.into(),
                todo: true,
                done,
                link: None,
            });
        } else if t.is_empty() {
            flush(&mut note, &mut out);
        } else {
            let t = t.trim_start_matches('#').trim();
            match &mut note {
                Some(n) => {
                    n.text.push('\n');
                    n.text.push_str(t);
                }
                None => {
                    note = Some(PadItem {
                        text: t.into(),
                        ..Default::default()
                    })
                }
            }
        }
    }
    flush(&mut note, &mut out);
    out
}

/// Items as markdown: to-dos as `- [ ]`/`- [x]` lines, notes as
/// paragraphs, each link named after what it points at.
pub fn pad_to_markdown(
    items: &[&PadItem],
    name_of: &dyn Fn(crate::ElementId) -> Option<String>,
) -> String {
    let mut todos = String::new();
    let mut notes = String::new();
    for i in items {
        let link = i
            .link
            .and_then(name_of)
            .map(|n| format!(" (\u{2192} {n})"))
            .unwrap_or_default();
        if i.todo {
            let mut lines = i.text.lines();
            let first = lines.next().unwrap_or("");
            todos.push_str(&format!(
                "- [{}] {first}{link}\n",
                if i.done { "x" } else { " " }
            ));
            for l in lines {
                todos.push_str(&format!("  {l}\n"));
            }
        } else {
            notes.push_str(&format!("{}{link}\n\n", i.text.trim_end()));
        }
    }
    let mut out = String::new();
    if !todos.is_empty() {
        out.push_str("## To-dos\n\n");
        out.push_str(&todos);
        out.push('\n');
    }
    if !notes.is_empty() {
        out.push_str("## Notes\n\n");
        out.push_str(&notes);
    }
    out.trim_end().to_string() + "\n"
}

impl ProjectInfo {
    /// The title-block fields this fills, by the names title blocks use:
    /// only those with a value.
    pub fn fields(&self) -> Vec<(&'static str, String)> {
        [
            ("job", &self.name),
            ("project", &self.name),
            ("project_number", &self.number),
            ("client", &self.client),
            ("status", &self.status),
            ("issue_date", &self.issue_date),
            ("address", &self.address),
            ("site", &self.site_name),
            ("building", &self.building_name),
        ]
        .into_iter()
        .filter(|(_, v)| !v.trim().is_empty())
        .map(|(k, v)| (k, v.trim().to_string()))
        .collect()
    }

    /// Nothing filled in.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// The project's georeference, when srid + easting + northing are
    /// all set: where the model origin (0,0) sits on the map, which way
    /// is north, and how high project zero is. This is the common
    /// understanding a linked GeoPackage and the model share: the file
    /// stays in its source CRS, features are projected into local
    /// metres once, at load time, through [`GeoPlacement::world_of`].
    pub fn placement(&self) -> Option<GeoPlacement> {
        Some(GeoPlacement {
            srid: self.srid.clone()?,
            easting: self.easting?,
            northing: self.northing?,
            rotation_deg: self.true_north,
            height: self.elevation,
        })
    }
}

/// A compass direction a picked line is to point in the model.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Cardinal {
    #[default]
    North,
    East,
    South,
    West,
}

impl Cardinal {
    /// The four of them, for menus.
    pub const ALL: [Cardinal; 4] = [
        Cardinal::North,
        Cardinal::East,
        Cardinal::South,
        Cardinal::West,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Cardinal::North => "North",
            Cardinal::East => "East",
            Cardinal::South => "South",
            Cardinal::West => "West",
        }
    }

    /// Its model-space angle, degrees from the +X axis.
    pub fn angle_deg(self) -> f64 {
        match self {
            Cardinal::North => 90.0,
            Cardinal::East => 0.0,
            Cardinal::South => -90.0,
            Cardinal::West => 180.0,
        }
    }
}

/// Where a project sits on the map: the CRS, the model origin in it,
/// the turn from model-up to grid north, and the height of project
/// zero above sea level. One rigid transform for the whole project
/// (standard BIM practice).
///
/// Convention: `rotation_deg` turns model-up towards grid north,
/// anticlockwise in degrees — the same `true_north` the drawings use.
/// The model +X axis then points at `(cos r, -sin r)` in world metres,
/// which is exactly `IfcMapConversion`'s XAxisAbscissa/Ordinate; its
/// Eastings/Northings/OrthogonalHeight are easting/northing/height.
#[derive(Clone, Debug, PartialEq)]
pub struct GeoPlacement {
    pub srid: String,
    pub easting: f64,
    pub northing: f64,
    pub rotation_deg: f64,
    pub height: f64,
}

impl GeoPlacement {
    /// Model metres to map metres.
    pub fn world_of(&self, model: DVec2) -> DVec2 {
        let t = self.rotation_deg.to_radians();
        let (c, s) = (t.cos(), t.sin());
        DVec2::new(
            self.easting + model.x * c + model.y * s,
            self.northing - model.x * s + model.y * c,
        )
    }

    /// Map metres back to model metres (the inverse transform).
    pub fn model_of(&self, world: DVec2) -> DVec2 {
        let t = self.rotation_deg.to_radians();
        let (c, s) = (t.cos(), t.sin());
        let (dx, dy) = (world.x - self.easting, world.y - self.northing);
        DVec2::new(dx * c - dy * s, dx * s + dy * c)
    }

    /// The same placement with its origin at `world`: that map point
    /// becomes model (0,0) — "call this parcel corner zero".
    pub fn with_origin_at(&self, world: DVec2) -> Self {
        Self {
            easting: world.x,
            northing: world.y,
            ..self.clone()
        }
    }

    /// The rotation that makes a direction that is `world_dir` on the
    /// map point `to` in the model: pick a line of the geography, and
    /// the project turns until it runs north, east, south or west. None
    /// for a zero-length direction.
    pub fn rotation_towards(&self, world_dir: DVec2, to: Cardinal) -> Option<f64> {
        if !world_dir.is_finite() || world_dir.length_squared() < 1e-18 {
            return None;
        }
        let want = to.angle_deg();
        let have = world_dir.y.atan2(world_dir.x).to_degrees();
        let d = (want - have).rem_euclid(360.0);
        Some(if d > 180.0 { d - 360.0 } else { d })
    }

    /// `(eastings, northings, height, x_abscissa, x_ordinate)` for
    /// `IfcMapConversion` (scale is always 1: both sides are metres).
    pub fn to_map_conversion(&self) -> (f64, f64, f64, f64, f64) {
        let t = self.rotation_deg.to_radians();
        (self.easting, self.northing, self.height, t.cos(), -t.sin())
    }

    /// The inverse: rotation recovered from the model-X direction.
    pub fn from_map_conversion(
        srid: String,
        eastings: f64,
        northings: f64,
        height: f64,
        x_abscissa: f64,
        x_ordinate: f64,
    ) -> Self {
        Self {
            srid,
            easting: eastings,
            northing: northings,
            // +0.0 normalises a -0.0 sweep to plain zero for display.
            rotation_deg: (-x_ordinate).atan2(x_abscissa).to_degrees() + 0.0,
            height,
        }
    }

    /// True north as a 2D direction in model coordinates, for
    /// `IfcGeometricRepresentationContext.TrueNorth`.
    pub fn true_north_dir(&self) -> DVec2 {
        let t = self.rotation_deg.to_radians();
        DVec2::new(-t.sin(), t.cos())
    }
}

/// Decimal degrees to IFC's (degrees, minutes, seconds, millionths):
/// the sign rides on the degrees, the rest stays non-negative.
pub fn decimal_to_dms(decimal: f64) -> (i32, u32, u32, u32) {
    let total = (decimal.abs() * 3_600_000_000.0).round() as i64;
    let deg = (total / 3_600_000_000) as i32 * decimal.signum() as i32;
    let rest = (total % 3_600_000_000) as u64;
    let min = (rest / 60_000_000) as u32;
    let sec = ((rest % 60_000_000) / 1_000_000) as u32;
    let millionths = (rest % 1_000_000) as u32;
    (deg, min, sec, millionths)
}

/// Back: IFC's compound angle to decimal degrees.
pub fn dms_to_decimal(deg: i32, min: u32, sec: u32, millionths: u32) -> f64 {
    let sign = if deg < 0 { -1.0 } else { 1.0 };
    sign * (deg.abs() as f64
        + min as f64 / 60.0
        + sec as f64 / 3600.0
        + millionths as f64 / 3_600_000_000.0)
}

/// The change that makes `info` the project's information: one undo
/// step, nothing when it is already so.
pub fn set_project(model: &crate::Model, info: ProjectInfo) -> crate::Transaction {
    let tx = crate::Transaction::new("Project information");
    match model.project_info() {
        Some((_, p)) if *p == info => tx,
        Some((id, _)) => {
            let Some(e) = model.get(id) else { return tx };
            let mut after = e.clone();
            after.kind = crate::ElementKind::Project(info);
            tx.replace(e.clone(), after)
        }
        None => tx.insert(crate::Element::new(
            "Project information",
            crate::ElementKind::Project(info),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_filled_fields_reach_the_title_blocks() {
        let p = ProjectInfo {
            name: "Maple Lane House".into(),
            client: "  ".into(),
            ..Default::default()
        };
        let f = p.fields();
        assert!(f.contains(&("job", "Maple Lane House".to_string())));
        assert!(f.contains(&("project", "Maple Lane House".to_string())));
        assert!(!f.iter().any(|(k, _)| *k == "client"));
        assert!(ProjectInfo::default().is_empty());
    }

    #[test]
    fn the_old_notepad_reads_as_items_and_writes_back_as_markdown() {
        let md = "- [x] Start a new project\n- [ ] testing a new note\n\nA note\nits second line";
        let items = pad_from_markdown(md);
        assert_eq!(items.len(), 3);
        assert!(items[0].todo && items[0].done);
        assert_eq!(items[2].title(), "A note");
        let refs: Vec<&PadItem> = items.iter().collect();
        let back = pad_to_markdown(&refs, &|_| None);
        assert!(back.contains("- [x] Start a new project"));
        assert!(back.contains("## Notes"));
        assert_eq!(
            pad_from_markdown(&back).iter().filter(|i| i.todo).count(),
            2
        );
    }

    #[test]
    fn setting_it_twice_keeps_one_element() {
        let mut doc = crate::document::Document::new();
        let a = ProjectInfo {
            name: "A".into(),
            ..Default::default()
        };
        doc.commit(set_project(doc.model(), a.clone())).unwrap();
        let b = ProjectInfo {
            name: "B".into(),
            ..a.clone()
        };
        doc.commit(set_project(doc.model(), b)).unwrap();
        let n = doc
            .model()
            .iter()
            .filter(|e| matches!(e.kind, crate::ElementKind::Project(_)))
            .count();
        assert_eq!(n, 1);
        assert_eq!(doc.model().project_info().unwrap().1.name, "B");
        assert!(
            set_project(doc.model(), doc.model().project_info().unwrap().1.clone())
                .ops
                .is_empty()
        );
    }

    fn placed() -> ProjectInfo {
        ProjectInfo {
            srid: Some("EPSG:28992".into()),
            // Domtoren, Utrecht, in RD metres.
            easting: Some(136331.0),
            northing: Some(455986.0),
            true_north: 0.0,
            elevation: 1.5,
            ..Default::default()
        }
    }

    #[test]
    fn placement_needs_srid_easting_and_northing() {
        assert!(placed().placement().is_some());
        assert!(ProjectInfo::default().placement().is_none());
        let mut p = placed();
        p.northing = None;
        assert!(p.placement().is_none());
    }

    #[test]
    fn a_picked_line_points_the_way_it_is_called() {
        use glam::DVec2;
        let g = placed().placement().unwrap();
        // a map-north road called North stays as it is
        let t = g.rotation_towards(DVec2::new(0.0, 1.0), Cardinal::North);
        assert!((t.unwrap()).abs() < 1e-9);
        // the same road called East turns the project −90°
        let t = g.rotation_towards(DVec2::new(0.0, 1.0), Cardinal::East).unwrap();
        assert!((t + 90.0).abs() < 1e-9);
        // and after that turn, the model really sees it running east:
        // the model direction is the rotation applied to the world one
        let (c, s) = (t.to_radians().cos(), t.to_radians().sin());
        let model = DVec2::new(0.0 * c - 1.0 * s, 0.0 * s + 1.0 * c);
        assert!((model.x - 1.0).abs() < 1e-9 && model.y.abs() < 1e-9);
        // an east-west line called South
        let t = g.rotation_towards(DVec2::new(1.0, 0.0), Cardinal::South).unwrap();
        assert!((t + 90.0).abs() < 1e-9);
        // with the project already turned, the current rotation counts
        let mut p = placed();
        p.true_north = 90.0;
        let g = p.placement().unwrap();
        // world east already points north there: the rotation stays 90
        let t = g.rotation_towards(DVec2::new(1.0, 0.0), Cardinal::North).unwrap();
        assert!((t - 90.0).abs() < 1e-9, "got {t}");
        // zero length asks for nothing
        assert!(g.rotation_towards(DVec2::ZERO, Cardinal::North).is_none());
    }

    #[test]
    fn zeroing_puts_a_picked_point_at_the_origin() {
        use glam::DVec2;
        let g = placed().placement().unwrap();
        let at = DVec2::new(136334.0, 455990.0);
        let zeroed = g.with_origin_at(at);
        assert_eq!(zeroed.srid, g.srid);
        assert_eq!(zeroed.rotation_deg, g.rotation_deg);
        assert!(zeroed.model_of(at).distance(DVec2::ZERO) < 1e-9);
    }

    #[test]
    fn world_of_is_origin_plus_rotated_metres() {
        let g = placed().placement().unwrap();
        let o = g.world_of(glam::DVec2::ZERO);
        assert_eq!((o.x, o.y), (136331.0, 455986.0));
        let p = g.world_of(glam::DVec2::new(10.0, 0.0));
        assert_eq!((p.x, p.y), (136341.0, 455986.0));
        // 90° anticlockwise: model +X points south, +Y points east.
        let mut p = placed();
        p.true_north = 90.0;
        let g = p.placement().unwrap();
        let x = g.world_of(glam::DVec2::new(1.0, 0.0));
        assert!((x.x - 136331.0).abs() < 1e-9 && (x.y - 455985.0).abs() < 1e-9);
        let y = g.world_of(glam::DVec2::new(0.0, 1.0));
        assert!((y.x - 136332.0).abs() < 1e-9 && (y.y - 455986.0).abs() < 1e-9);
        // Round trip, all quadrants.
        for (mx, my) in [(-3.0, 4.0), (12.5, -7.25), (0.0, 0.0)] {
            let m = glam::DVec2::new(mx, my);
            let back = g.model_of(g.world_of(m));
            assert!((back - m).length() < 1e-9, "{m:?}");
        }
    }

    #[test]
    fn map_conversion_round_trips_through_model_x() {
        let g = placed().placement().unwrap();
        let (e, n, h, xa, xo) = g.to_map_conversion();
        assert_eq!((e, n, h), (136331.0, 455986.0, 1.5));
        assert!((xa - 1.0).abs() < 1e-12 && xo.abs() < 1e-12);
        let back = GeoPlacement::from_map_conversion("EPSG:28992".into(), e, n, h, xa, xo);
        assert!((back.rotation_deg - 0.0).abs() < 1e-9);
        assert_eq!((back.easting, back.northing, back.height), (e, n, h));
        // A 30° turn survives the axis round trip.
        let mut p = placed();
        p.true_north = 30.0;
        let g = p.placement().unwrap();
        let (_, _, _, xa, xo) = g.to_map_conversion();
        let back = GeoPlacement::from_map_conversion("EPSG:28992".into(), e, n, h, xa, xo);
        assert!((back.rotation_deg - 30.0).abs() < 1e-9);
        assert!((g.true_north_dir() - glam::DVec2::new(-0.5, 0.8660254)).length() < 1e-6);
    }

    #[test]
    fn dms_round_trips_dutch_coordinates() {
        assert_eq!(decimal_to_dms(52.0907), (52, 5, 26, 520000));
        assert_eq!(decimal_to_dms(5.1214), (5, 7, 17, 40000));
        for d in [52.0907, 5.1214, 0.0, -33.865143] {
            let (deg, min, sec, micro) = decimal_to_dms(d);
            assert!(
                (dms_to_decimal(deg, min, sec, micro) - d).abs() < 1e-7,
                "{d}"
            );
        }
    }
}
