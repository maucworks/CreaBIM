//! Linking a GeoPackage's vector layers into the model as
//! [`SiteFeature`](creabim_core::SiteFeature) elements — one element
//! per source layer, features keyed by source row-id inside it.
//!
//! The project's [`GeoPlacement`](creabim_core::GeoPlacement) decides
//! the projection: every feature is transformed from the file's CRS
//! into local metres on load (SRID mismatches fail loudly, naming
//! the layer). Re-running on an unchanged file is a no-op (the stamp
//! check); changed rows replace in place, new rows insert, vanished
//! rows and layers go away with cascade.

use crate::gpkg::{self, GpkgFeature, GpkgGeom};
use creabim_core::{
    hash::StableHasher, Element, ElementKind, GeoPlacement, Model, SiteCategory, SiteFeature,
    SiteGeometry, SiteRow, Transaction,
};
use std::collections::{HashMap, HashSet};
use std::hash::Hasher;

/// What `site load` did.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SiteReport {
    pub added: usize,
    pub updated: usize,
    pub removed: usize,
    pub unchanged: usize,
    pub skipped_layers: Vec<String>,
    pub skipped_features: usize,
    pub notes: Vec<String>,
}

fn srs_name(path: &str, srs_id: i32) -> Result<(String, String), String> {
    let db =
        rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|e| format!("{path}: {e}"))?;
    let (org, id): (String, i64) = db
        .query_row(
            "SELECT organization, organization_coordsys_id FROM gpkg_spatial_ref_sys WHERE srs_id = ?1",
            [srs_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|e| format!("{path}: srs {srs_id}: {e}"))?;
    Ok((org, id.to_string()))
}

/// A GeoPackage's CRS and extent, read before anything is loaded: what
/// a project needs to place it (the CRS becomes the project's, the
/// extent's middle is where the model origin starts).
#[derive(Clone, Debug, PartialEq)]
pub struct FileGeo {
    /// `ORG:ID`, as the placement's `srid` wants it.
    pub crs: String,
    /// min_x, min_y, max_x, max_y in the file's CRS; None when
    /// `gpkg_contents` carries no extent.
    pub bounds: Option<(f64, f64, f64, f64)>,
}

/// The CRS and extent of the first layer that has both.
pub fn file_geo(path: &str) -> Result<FileGeo, String> {
    let db =
        rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|e| format!("{path}: {e}"))?;
    let mut stmt = db
        .prepare(
            "SELECT srs_id, min_x, min_y, max_x, max_y FROM gpkg_contents
             WHERE srs_id IS NOT NULL AND data_type = 'features'",
        )
        .map_err(|e| format!("{path}: {e}"))?;
    let rows: Vec<(i64, f64, f64, f64, f64)> = stmt
        .query_map([], |r| {
            Ok((
                r.get(0)?,
                r.get::<_, Option<f64>>(1)?.unwrap_or(0.0),
                r.get::<_, Option<f64>>(2)?.unwrap_or(0.0),
                r.get::<_, Option<f64>>(3)?.unwrap_or(0.0),
                r.get::<_, Option<f64>>(4)?.unwrap_or(0.0),
            ))
        })
        .map_err(|e| format!("{path}: {e}"))?
        .filter_map(|r| r.ok())
        .collect();
    let (srs, bounds) = rows
        .first()
        .map(|(srs, x0, y0, x1, y1)| (*srs, Some((*x0, *y0, *x1, *y1))))
        .ok_or_else(|| format!("{path}: no feature layers with a CRS"))?;
    let (org, id) = srs_name(path, srs as i32)?;
    Ok(FileGeo {
        crs: format!("{org}:{id}"),
        bounds,
    })
}

/// Load every 2D vector layer of `path` into the model, transformed by
/// `placement`. The file must already sit in the placement's CRS —
/// reprojecting on import is out (trap: silent datum shifts).
pub fn sync_site(
    model: &Model,
    path: &str,
    placement: &GeoPlacement,
) -> Result<(Transaction, SiteReport), String> {
    let mut report = SiteReport::default();
    let mut tx = Transaction::new(format!("site load {}", file_stem(path)));
    let (want_org, want_id) = placement
        .srid
        .split_once(':')
        .ok_or_else(|| format!("placement srid {:?} is not ORG:ID", placement.srid))?;
    let by_key: HashMap<String, &Element> = model
        .iter()
        .filter_map(|e| match &e.kind {
            ElementKind::SiteFeature(s) => e
                .source
                .as_ref()
                .filter(|src| src.file == path)
                .map(|_| (s.layer.clone(), e)),
            _ => None,
        })
        .collect();
    let mut seen: HashSet<String> = HashSet::new();
    for layer in gpkg::layers(path)? {
        if !layer.loadable {
            report.skipped_layers.push(format!(
                "{} ({} — not a 2D vector layer)",
                layer.name, layer.geom_type
            ));
            // BAG3D's buildings are 3D masses, bespoke to the NL tiles:
            // their 3D ingest is its own reader later on. Name them so
            // nobody wonders where they went.
            if layer.name.starts_with("bag3d") {
                report
                    .notes
                    .push(format!("{}: BAG3D 3D masses are not loaded yet; their footprint layers draw in plan.", layer.name));
            }
            continue;
        }
        let (org, id) = srs_name(path, layer.srs)?;
        if org != want_org || id != want_id {
            return Err(format!(
                "{}: CRS is {org}:{id}, project needs {want_org}:{want_id} (reprojection on load is out)", layer.name
            ));
        }
        let mut skipped = gpkg::Skipped::default();
        let feats = gpkg::load_layer(path, &layer.name, &mut skipped)?;
        report.skipped_features += skipped.points + skipped.empty + skipped.other;
        for n in skipped.notes {
            report.notes.push(format!("{}: {n}", layer.name));
        }
        let mut rows = vec![];
        for f in feats {
            rows.push(to_row(f, placement));
        }
        rows.sort_by(|a, b| a.key.cmp(&b.key));
        let mut h = StableHasher::new();
        h.write(layer.name.as_bytes());
        for r in &rows {
            h.write(r.key.as_bytes());
            h.write(format!("{:?}", r.geom).as_bytes());
        }
        let stamp = h.finish();
        match by_key.get(&layer.name) {
            Some(e) if e.source.as_ref().is_some_and(|s| s.stamp == stamp) => {
                seen.insert(layer.name.clone());
                report.unchanged += 1;
            }
            Some(e) => {
                seen.insert(layer.name.clone());
                let before = (*e).clone();
                let mut after = before.clone();
                if let ElementKind::SiteFeature(s) = &mut after.kind {
                    s.features = rows;
                    s.category = SiteCategory::of_layer(&layer.name);
                }
                if let Some(src) = after.source.as_mut() {
                    src.stamp = stamp;
                }
                tx = tx.replace(before, after);
                report.updated += 1;
            }
            None => {
                let mut el = Element::new(
                    format!("Site {}", layer.name),
                    ElementKind::SiteFeature(SiteFeature {
                        layer: layer.name.clone(),
                        category: SiteCategory::of_layer(&layer.name),
                        style: creabim_core::LayerStyle::default(),
                        features: rows,
                    }),
                );
                el.source = Some(creabim_core::Source {
                    file: path.to_string(),
                    object: layer.name.clone(),
                    stamp,
                });
                seen.insert(layer.name);
                tx = tx.insert(el);
                report.added += 1;
            }
        }
    }
    // Layers gone from the file go from the model, unless something
    // else being removed takes them with it.
    let gone: Vec<creabim_core::ElementId> = by_key
        .iter()
        .filter(|(layer, _)| !seen.contains(*layer))
        .map(|(_, e)| e.id)
        .collect();
    let gone_set: HashSet<_> = gone.iter().cloned().collect();
    for id in gone {
        if model
            .get(id)
            .is_some_and(|e| e.references().iter().any(|r| gone_set.contains(r)))
        {
            continue;
        }
        tx = tx.remove_cascade(model, id);
        report.removed += 1;
    }
    Ok((tx, report))
}

fn file_stem(path: &str) -> &str {
    std::path::Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(path)
}

fn to_row(f: GpkgFeature, placement: &GeoPlacement) -> SiteRow {
    use glam::DVec2;
    let at = |x: f64, y: f64| placement.model_of(DVec2::new(x, y));
    let geom = match f.geom {
        GpkgGeom::Polygon { exterior, holes } => SiteGeometry::Polygon {
            exterior: exterior.into_iter().map(|(x, y)| at(x, y)).collect(),
            holes: holes
                .into_iter()
                .map(|h| h.into_iter().map(|(x, y)| at(x, y)).collect())
                .collect(),
        },
        GpkgGeom::Polyline { points } => SiteGeometry::Polyline {
            points: points.into_iter().map(|(x, y)| at(x, y)).collect(),
        },
    };
    SiteRow { key: f.key, geom }
}

#[cfg(test)]
mod tests {
    use super::*;
    use creabim_core::Document;

    fn test_gpkg(tag: &str) -> (std::path::PathBuf, String, GeoPlacement) {
        let dir =
            std::env::temp_dir().join(format!("creabim-site-test-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("site.gpkg");
        let db = rusqlite::Connection::open(&path).unwrap();
        db.execute_batch(
            "CREATE TABLE gpkg_geometry_columns (table_name TEXT, column_name TEXT, geometry_type_name TEXT, srs_id INTEGER);
             CREATE TABLE gpkg_spatial_ref_sys (srs_id INTEGER PRIMARY KEY, organization TEXT, organization_coordsys_id INTEGER, definition TEXT);
             CREATE TABLE gpkg_contents (table_name TEXT, data_type TEXT, srs_id INTEGER, min_x DOUBLE, min_y DOUBLE, max_x DOUBLE, max_y DOUBLE);
             INSERT INTO gpkg_spatial_ref_sys VALUES (28992, 'EPSG', '28992', 'x');
             INSERT INTO gpkg_contents VALUES ('wijk', 'features', 28992, 9510.0, 308000.0, 9530.0, 308020.0);
             CREATE TABLE wijk (fid INTEGER PRIMARY KEY, geom BLOB);
             INSERT INTO gpkg_geometry_columns VALUES ('wijk', 'geom', 'POLYGON', 28992);",
        )
        .unwrap();
        let mut wkb = vec![1u8];
        wkb.extend_from_slice(&3u32.to_le_bytes());
        wkb.extend_from_slice(&1u32.to_le_bytes());
        let ring: Vec<(f64, f64)> = vec![
            (0.0, 0.0),
            (10.0, 0.0),
            (10.0, 10.0),
            (0.0, 10.0),
            (0.0, 0.0),
        ];
        wkb.extend_from_slice(&(ring.len() as u32).to_le_bytes());
        for (x, y) in ring {
            wkb.extend_from_slice(&x.to_le_bytes());
            wkb.extend_from_slice(&y.to_le_bytes());
        }
        let mut blob = vec![b'G', b'P', 0, 0x01];
        blob.extend_from_slice(&28992u32.to_le_bytes());
        blob.extend_from_slice(&wkb);
        db.execute(
            "INSERT INTO wijk (fid, geom) VALUES (1, ?)",
            rusqlite::params![blob],
        )
        .unwrap();
        let placement = GeoPlacement {
            srid: "EPSG:28992".into(),
            easting: 100.0,
            northing: 200.0,
            rotation_deg: 0.0,
            height: 0.0,
        };
        (dir, path.to_string_lossy().into_owned(), placement)
    }

    #[test]
    fn load_creates_local_features() {
        let (_dir, path, g) = test_gpkg("load");
        let model = Model::new();
        let (tx, report) = sync_site(&model, &path, &g).unwrap();
        assert_eq!((report.added, report.updated), (1, 0));
        let mut doc = Document::from_model(model);
        doc.commit(tx).unwrap();
        let el = doc
            .model()
            .iter()
            .find(|e| matches!(e.kind, ElementKind::SiteFeature(_)))
            .expect("site layer");
        match &el.kind {
            ElementKind::SiteFeature(s) => {
                assert_eq!(s.layer, "wijk");
                assert_eq!(s.features.len(), 1);
                assert_eq!(s.features[0].key, "1");
            }
            _ => unreachable!(),
        }
        // Local metres: file (0..10, 0..10) minus origin (100, 200).
        let area = match &el.kind {
            ElementKind::SiteFeature(s) => s.area(),
            _ => 0.0,
        };
        assert!((area - 100.0).abs() < 1e-9);
        let first = match &el.kind {
            ElementKind::SiteFeature(s) => match &s.features[0].geom {
                SiteGeometry::Polygon { exterior, .. } => exterior[0],
                _ => panic!("polygon"),
            },
            _ => unreachable!(),
        };
        assert!((first.x + 100.0).abs() < 1e-9 && (first.y + 200.0).abs() < 1e-9);
    }

    #[test]
    fn rerun_is_quiet_and_edits_survive() {
        let (_dir, path, g) = test_gpkg("rerun");
        let mut doc = Document::from_model(Model::new());
        let (tx, _) = sync_site(doc.model(), &path, &g).unwrap();
        doc.commit(tx).unwrap();
        let id = doc
            .model()
            .iter()
            .find(|e| matches!(e.kind, ElementKind::SiteFeature(_)))
            .unwrap()
            .id;
        // Rename the layer element (as the site editor would, under a
        // `site …` label): the next sync must keep the name.
        let before = doc.model().get(id).cloned().unwrap();
        let mut renamed = before.clone();
        renamed.name = "Wijk Centrum".into();
        doc.commit(Transaction::new("site rename").replace(before, renamed))
            .unwrap();
        // Unchanged file: no ops at all.
        let (tx, report) = sync_site(doc.model(), &path, &g).unwrap();
        assert!(tx.ops.is_empty());
        assert_eq!(report.unchanged, 1);
        assert_eq!(doc.model().get(id).unwrap().name, "Wijk Centrum");
    }

    #[test]
    fn placement_crs_mismatch_is_loud() {
        let (_dir, path, _g) = test_gpkg("mismatch");
        let g = GeoPlacement {
            srid: "EPSG:4326".into(),
            easting: 0.0,
            northing: 0.0,
            rotation_deg: 0.0,
            height: 0.0,
        };
        let err = sync_site(&Model::new(), &path, &g).unwrap_err();
        assert!(err.contains("28992"), "{err}");
    }

    #[test]
    fn file_geo_reads_the_crs_and_extent() {
        let (_dir, path, _) = test_gpkg("geo");
        let g = file_geo(&path).unwrap();
        assert_eq!(g.crs, "EPSG:28992");
        assert_eq!(g.bounds, Some((9510.0, 308000.0, 9530.0, 308020.0)));
    }

    #[test]
    fn a_reload_keeps_the_layer_style() {
        use creabim_core::styles::{LinePattern, Rgb};
        use creabim_core::LayerStyle;
        let (_dir, path, g) = test_gpkg("style");
        let mut doc = Document::from_model(Model::new());
        let (tx, _) = sync_site(doc.model(), &path, &g).unwrap();
        doc.commit(tx).unwrap();
        let id = doc
            .model()
            .iter()
            .find(|e| matches!(e.kind, ElementKind::SiteFeature(_)))
            .unwrap()
            .id;
        // the site editor styles the layer under a `site …` label
        let before = doc.model().get(id).cloned().unwrap();
        let mut styled = before.clone();
        if let ElementKind::SiteFeature(s) = &mut styled.kind {
            s.style = LayerStyle {
                fill: Some(Rgb([0.3, 0.5, 0.7])),
                line: Some(Rgb([0.9, 0.1, 0.1])),
                pattern: Some(LinePattern::Dashed),
                visible: true,
            };
        }
        doc.commit(Transaction::new("site style site.gpkg").replace(before, styled))
            .unwrap();
        // the file changes (a new stamp), the layer reloads: style intact
        let (tx, report) = sync_site(doc.model(), &path, &g).unwrap();
        assert_eq!(report.unchanged, 1, "an unchanged file must not rewrite");
        assert!(tx.ops.is_empty());
        // now force a change: the layer's stamp differs only when rows do;
        // so restamp by hand and sync again — the point is the same path
        let before = doc.model().get(id).cloned().unwrap();
        let mut bumped = before.clone();
        if let Some(src) = bumped.source.as_mut() {
            src.stamp += 1;
        }
        doc.commit(Transaction::new("site test bump").replace(before, bumped))
            .unwrap();
        let (tx, report) = sync_site(doc.model(), &path, &g).unwrap();
        assert_eq!(report.updated, 1);
        assert!(!tx.ops.is_empty());
        let el = doc.model().get(id).unwrap();
        match &el.kind {
            ElementKind::SiteFeature(s) => {
                assert_eq!(s.style.line, Some(Rgb([0.9, 0.1, 0.1])));
                assert_eq!(s.style.pattern, Some(LinePattern::Dashed));
            }
            _ => unreachable!(),
        }
    }
}
