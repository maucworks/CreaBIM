//! Reading site geography out of a GeoPackage: layer inventory plus
//! 2D polygon/polyline features with source row-ids. Files stay in
//! their source CRS; projection to the project's local metres happens
//! at load time (see `site::sync_site`), never in here.
//!
//! Only SQLite + WKB are involved (no GDAL): the header is 8 bytes
//! (`GP`, version, flags, srs_id) plus the envelope the flags describe,
//! then a standard WKB geometry. geozero parses the body; this module
//! only strips the envelope.

use geozero::{error::GeozeroError, wkb::Wkb, GeomProcessor, GeozeroGeometry};
use rusqlite::{Connection, OpenFlags};

/// One readable layer: its name, geometry kind and row count.
#[derive(Clone, Debug, PartialEq)]
pub struct GpkgLayer {
    pub name: String,
    pub geom_type: String,
    pub srs: i32,
    pub count: usize,
    /// 2D polygon/polyline layers load; anything else is reported and skipped.
    pub loadable: bool,
}

/// A feature's 2D shape, in the file's CRS.
#[derive(Clone, Debug, PartialEq)]
pub enum GpkgGeom {
    Polygon {
        exterior: Vec<(f64, f64)>,
        holes: Vec<Vec<(f64, f64)>>,
    },
    Polyline {
        points: Vec<(f64, f64)>,
    },
}

/// One row: its sync key plus shape. Plain rows key by row-id;
/// multipolygon parts key as `{rowid}#{part}` (WKB order, stable
/// across reads of the same file), so a changed part syncs
/// surgically instead of replacing the whole row.
#[derive(Clone, Debug, PartialEq)]
pub struct GpkgFeature {
    pub key: String,
    pub geom: GpkgGeom,
}

/// What `load_layer` left out, for the report.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Skipped {
    pub points: usize,
    pub empty: usize,
    pub other: usize,
    /// Why non-point rows failed (capped): loud, never silent.
    pub notes: Vec<String>,
}

/// Layers with a geometry column, most rows first.
pub fn layers(path: &str) -> Result<Vec<GpkgLayer>, String> {
    let db = open(path)?;
    let mut out = vec![];
    let mut stmt = db
        .prepare(
            "SELECT table_name, geometry_type_name, srs_id FROM gpkg_geometry_columns ORDER BY table_name",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i32>(2)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    for row in rows {
        let (name, geom_type, srs) = row.map_err(|e| e.to_string())?;
        let count: i64 = db
            .query_row(&format!("SELECT COUNT(*) FROM \"{name}\""), [], |r| {
                r.get(0)
            })
            .map_err(|e| format!("{name}: {e}"))?;
        let count = count.max(0) as usize;
        let loadable = matches!(
            geom_type.as_str(),
            "POLYGON" | "MULTIPOLYGON" | "LINESTRING" | "MULTILINESTRING"
        );
        out.push(GpkgLayer {
            name,
            geom_type,
            srs,
            count,
            loadable,
        });
    }
    out.sort_by_key(|a| std::cmp::Reverse(a.count));
    Ok(out)
}

/// Every 2D polygon/polyline row of one layer, in rowid order.
pub fn load_layer(
    path: &str,
    layer: &str,
    skipped: &mut Skipped,
) -> Result<Vec<GpkgFeature>, String> {
    let db = open(path)?;
    let geom_col: String = db
        .query_row(
            "SELECT column_name FROM gpkg_geometry_columns WHERE table_name = ?1",
            [layer],
            |r| r.get(0),
        )
        .map_err(|e| format!("{layer}: {e}"))?;
    let mut stmt = db
        .prepare(&format!(
            "SELECT rowid, \"{geom_col}\" FROM \"{layer}\" ORDER BY rowid"
        ))
        .map_err(|e| format!("{layer}: {e}"))?;
    let mut out = vec![];
    let mut rows = stmt.query([]).map_err(|e| format!("{layer}: {e}"))?;
    while let Some(row) = rows.next().map_err(|e| format!("{layer}: {e}"))? {
        let rowid: i64 = row.get(0).map_err(|e| format!("{layer}: {e}"))?;
        let blob: Vec<u8> = row.get(1).map_err(|e| format!("{layer}: {e}"))?;
        match read_geom(&blob) {
            Ok(geoms) if geoms.len() == 1 => out.push(GpkgFeature {
                key: rowid.to_string(),
                geom: geoms.into_iter().next().unwrap(),
            }),
            Ok(geoms) if !geoms.is_empty() => {
                for (i, geom) in geoms.into_iter().enumerate() {
                    out.push(GpkgFeature {
                        key: format!("{rowid}#{i}"),
                        geom,
                    });
                }
            }
            Ok(_) => skipped.empty += 1,
            Err(ReadErr::Points) => skipped.points += 1,
            Err(ReadErr::Other(e)) => {
                skipped.other += 1;
                if skipped.notes.len() < 5 {
                    skipped.notes.push(format!("row {rowid}: {e}"));
                }
            }
        }
    }
    Ok(out)
}

fn open(path: &str) -> Result<Connection, String> {
    Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| format!("{path}: {e}"))
}

struct PointsOnly;
#[derive(Debug)]
enum ReadErr {
    Points,
    Other(String),
}

impl From<PointsOnly> for ReadErr {
    fn from(_: PointsOnly) -> Self {
        ReadErr::Points
    }
}

/// Strip the GPKG header, returning the WKB body (or `None` for an
/// empty geometry). `Err(PointsOnly)` marks point/multipoint blobs:
/// counted separately, never an error.
fn split_blob(blob: &[u8]) -> Result<Option<&[u8]>, PointsOnly> {
    if blob.len() < 8 || &blob[0..2] != b"GP" {
        return Err(PointsOnly);
    }
    let flags = blob[3];
    // Envelope size from the indicator bits (E is reserved).
    let env: usize = match (flags >> 1) & 0x7 {
        0 => 0,
        1 => 32,
        2 | 3 => 48,
        _ => 48,
    };
    let body = blob.get(8 + env..).ok_or(PointsOnly)?;
    // Peek the WKB type to sort out points early (1 = point,
    // 4 = multipoint, +1000/2000/3000 = Z/M/ZM).
    if body.len() >= 5 {
        let le = body[0] == 1;
        let mut kind = [body[1], body[2], body[3], body[4]];
        if !le {
            kind.reverse();
        }
        let kind = u32::from_ne_bytes(kind) % 1000;
        if kind == 1 || kind == 4 {
            return Err(PointsOnly);
        }
        if body.len() == 5 {
            return Ok(None);
        }
    }
    Ok(Some(body))
}

fn read_geom(blob: &[u8]) -> Result<Vec<GpkgGeom>, ReadErr> {
    let body = split_blob(blob)?;
    let Some(body) = body else {
        return Ok(vec![]);
    };
    let mut shapes = Shapes::default();
    Wkb(body)
        .process_geom(&mut shapes)
        .map_err(|e| ReadErr::Other(e.to_string()))?;
    Ok(shapes.finish())
}

#[derive(Default)]
struct Shapes {
    polygons: Vec<PolyRings>,
    polylines: Vec<Ring>,
    ring: Ring,
    in_polygon: bool,
    unsupported: bool,
}

/// One polygon: exterior ring plus holes, as point lists.
type PolyRings = (Ring, Vec<Ring>);
/// A ring: plain (x, y) points in file CRS.
type Ring = Vec<(f64, f64)>;

impl Shapes {
    /// Finished polygons, degenerates dropped. Multipolygon parts come
    /// out as separate entries (the caller keys them `{rowid}#{part}`).
    fn finish(mut self) -> Vec<GpkgGeom> {
        if self.unsupported {
            return vec![];
        }
        // Drop degenerate rings (fewer than 3 distinct corners survive
        // validation nowhere, so they never enter the model).
        let mut out = vec![];
        for (mut exterior, holes) in std::mem::take(&mut self.polygons) {
            dedup_close(&mut exterior);
            if exterior.len() < 3 {
                continue;
            }
            let mut kept = vec![];
            for mut h in holes {
                dedup_close(&mut h);
                if h.len() >= 3 {
                    kept.push(h);
                }
            }
            out.push(GpkgGeom::Polygon {
                exterior,
                holes: kept,
            });
        }
        if out.is_empty() && self.polylines.len() == 1 {
            return vec![GpkgGeom::Polyline {
                points: self.polylines.pop().unwrap(),
            }];
        }
        out
    }
}

/// Drop a closing duplicate and consecutive duplicates: WKB rings
/// repeat the first point last, and the model stores open rings.
fn dedup_close(pts: &mut Vec<(f64, f64)>) {
    pts.dedup_by(|a, b| (a.0 - b.0).abs() < 1e-9 && (a.1 - b.1).abs() < 1e-9);
    if pts.len() > 1 {
        let (f, l) = (pts[0], pts[pts.len() - 1]);
        if (f.0 - l.0).abs() < 1e-9 && (f.1 - l.1).abs() < 1e-9 {
            pts.pop();
        }
    }
}

impl GeomProcessor for Shapes {
    fn point_begin(&mut self, _idx: usize) -> Result<(), GeozeroError> {
        Ok(())
    }

    fn point_end(&mut self, _idx: usize) -> Result<(), GeozeroError> {
        self.unsupported = true;
        Ok(())
    }

    fn xy(&mut self, x: f64, y: f64, _idx: usize) -> Result<(), GeozeroError> {
        self.ring.push((x, y));
        Ok(())
    }

    fn linestring_begin(
        &mut self,
        _tagged: bool,
        _size: usize,
        _idx: usize,
    ) -> Result<(), GeozeroError> {
        self.ring.clear();
        Ok(())
    }

    fn linestring_end(&mut self, _tagged: bool, _idx: usize) -> Result<(), GeozeroError> {
        // Polygon rings arrive as linestrings: the first fills the
        // exterior, the rest are holes. Standalone linestrings are
        // polylines.
        let ring = std::mem::take(&mut self.ring);
        if self.in_polygon {
            if let Some((exterior, holes)) = self.polygons.last_mut() {
                if exterior.is_empty() {
                    *exterior = ring;
                } else {
                    holes.push(ring);
                }
            }
        } else if ring.len() >= 2 {
            self.polylines.push(ring);
        }
        Ok(())
    }

    fn polygon_begin(
        &mut self,
        _tagged: bool,
        _size: usize,
        _idx: usize,
    ) -> Result<(), GeozeroError> {
        self.in_polygon = true;
        self.polygons.push((vec![], vec![]));
        Ok(())
    }

    fn polygon_end(&mut self, _tagged: bool, _idx: usize) -> Result<(), GeozeroError> {
        self.in_polygon = false;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn wkb_point(x: f64, y: f64, out: &mut Vec<u8>) {
        out.extend_from_slice(&x.to_le_bytes());
        out.extend_from_slice(&y.to_le_bytes());
    }

    fn wkb_ring(pts: &[(f64, f64)], out: &mut Vec<u8>) {
        let mut closed = pts.to_vec();
        closed.push(pts[0]);
        out.extend_from_slice(&(closed.len() as u32).to_le_bytes());
        for (x, y) in closed {
            wkb_point(x, y, out);
        }
    }

    fn wkb_polygon(rings: &[Vec<(f64, f64)>]) -> Vec<u8> {
        let mut out = vec![1u8];
        out.extend_from_slice(&3u32.to_le_bytes());
        out.extend_from_slice(&(rings.len() as u32).to_le_bytes());
        for r in rings {
            wkb_ring(r, &mut out);
        }
        out
    }

    fn gpkg_blob(wkb: &[u8]) -> Vec<u8> {
        let mut blob = vec![b'G', b'P', 0, 0x01];
        blob.extend_from_slice(&28992u32.to_le_bytes());
        blob.extend_from_slice(wkb);
        blob
    }

    fn test_db(tag: &str) -> (std::path::PathBuf, String) {
        let dir =
            std::env::temp_dir().join(format!("creabim-gpkg-test-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.gpkg");
        let db = Connection::open(&path).unwrap();
        db.execute_batch(
            "CREATE TABLE gpkg_geometry_columns (table_name TEXT, column_name TEXT, geometry_type_name TEXT, srs_id INTEGER);
             CREATE TABLE gpkg_spatial_ref_sys (srs_id INTEGER PRIMARY KEY, organization TEXT, organization_coordsys_id INTEGER, definition TEXT);
             INSERT INTO gpkg_spatial_ref_sys VALUES (28992, 'EPSG', 28992, 'unused');
             CREATE TABLE parcels (fid INTEGER PRIMARY KEY, geom BLOB);
             INSERT INTO gpkg_geometry_columns VALUES ('parcels', 'geom', 'POLYGON', 28992);
             CREATE TABLE lines (fid INTEGER PRIMARY KEY, geom BLOB);
             INSERT INTO gpkg_geometry_columns VALUES ('lines', 'geom', 'LINESTRING', 28992);
             CREATE TABLE labels (fid INTEGER PRIMARY KEY, geom BLOB);
             INSERT INTO gpkg_geometry_columns VALUES ('labels', 'geom', 'POINT', 28992);",
        )
        .unwrap();
        // Square with a square hole; triangle; open polyline; point.
        let sq =
            |x0: f64, y0: f64, s: f64| vec![(x0, y0), (x0 + s, y0), (x0 + s, y0 + s), (x0, y0 + s)];
        let hole = vec![(1.0, 1.0), (2.0, 1.0), (2.0, 2.0)];
        db.execute(
            "INSERT INTO parcels (fid, geom) VALUES (7, ?)",
            rusqlite::params![gpkg_blob(&wkb_polygon(&[sq(0.0, 0.0, 4.0), hole]))],
        )
        .unwrap();
        let mut multi = vec![1u8];
        multi.extend_from_slice(&6u32.to_le_bytes());
        multi.extend_from_slice(&2u32.to_le_bytes());
        multi.extend_from_slice(&wkb_polygon(&[sq(100.0, 100.0, 4.0)]));
        multi.extend_from_slice(&wkb_polygon(&[sq(200.0, 200.0, 4.0)]));
        db.execute(
            "INSERT INTO parcels (fid, geom) VALUES (8, ?)",
            rusqlite::params![gpkg_blob(&multi)],
        )
        .unwrap();
        let mut line = vec![1u8];
        line.extend_from_slice(&2u32.to_le_bytes());
        line.extend_from_slice(&3u32.to_le_bytes());
        for (x, y) in [(0.0, 0.0), (5.0, 0.0), (5.0, 5.0)] {
            wkb_point(x, y, &mut line);
        }
        db.execute(
            "INSERT INTO lines (fid, geom) VALUES (3, ?)",
            rusqlite::params![gpkg_blob(&line)],
        )
        .unwrap();
        let mut pt = vec![1u8];
        pt.extend_from_slice(&1u32.to_le_bytes());
        wkb_point(1.0, 2.0, &mut pt);
        db.execute(
            "INSERT INTO labels (fid, geom) VALUES (1, ?)",
            rusqlite::params![gpkg_blob(&pt)],
        )
        .unwrap();
        (dir, path.to_string_lossy().into_owned())
    }

    #[test]
    fn layers_lists_and_marks_loadable() {
        let (_dir, path) = test_db("layers");
        let layers = super::layers(&path).unwrap();
        assert_eq!(layers.len(), 3);
        let poly = layers.iter().find(|l| l.name == "parcels").unwrap();
        assert!(poly.loadable);
        assert_eq!(poly.count, 2);
        let pt = layers.iter().find(|l| l.name == "labels").unwrap();
        assert!(!pt.loadable);
    }

    #[test]
    fn polygons_keep_holes_and_flatten_multis() {
        let (_dir, path) = test_db("polys");
        let mut skipped = Skipped::default();
        let feats = super::load_layer(&path, "parcels", &mut skipped).unwrap();
        // fid 7: polygon with hole; fid 8: multipolygon of two squares.
        assert_eq!(feats.len(), 3);
        assert_eq!(feats[0].key, "7");
        match &feats[0].geom {
            GpkgGeom::Polygon { exterior, holes } => {
                assert_eq!(exterior.len(), 4, "closing duplicate dropped");
                assert_eq!(holes.len(), 1);
                assert_eq!(holes[0].len(), 3);
            }
            other => panic!("expected polygon, got {other:?}"),
        }
        assert_eq!(feats[1].key, "8#0");
        assert_eq!(feats[2].key, "8#1");
        match &feats[2].geom {
            GpkgGeom::Polygon { exterior, .. } => {
                assert!(exterior.iter().any(|&(x, _)| (x - 200.0).abs() < 1e-9));
            }
            other => panic!("expected polygon part, got {other:?}"),
        }
        assert_eq!((skipped.points, skipped.empty, skipped.other), (0, 0, 0));
    }

    #[test]
    fn polylines_read_and_points_counted() {
        let (_dir, path) = test_db("lines");
        let mut skipped = Skipped::default();
        let feats = super::load_layer(&path, "lines", &mut skipped).unwrap();
        assert_eq!(feats.len(), 1);
        match &feats[0].geom {
            GpkgGeom::Polyline { points } => assert_eq!(points.len(), 3),
            other => panic!("expected polyline, got {other:?}"),
        }
        let mut skipped = Skipped::default();
        let feats = super::load_layer(&path, "labels", &mut skipped).unwrap();
        assert!(feats.is_empty());
        assert_eq!(skipped.points, 1);
    }
}
