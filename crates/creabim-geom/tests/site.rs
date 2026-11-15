//! Site layers on the plan: polygons fill with the category hatch
//! and draw their outline; polylines draw as light projection lines.

use creabim_core::*;
use creabim_geom::plan::{plan_view, Pen, PlanItem};
use glam::DVec2;

fn site_model() -> (Document, ElementId) {
    let mut doc = Document::new();
    let l = Element::new(
        "L0",
        ElementKind::Level(Level {
            elevation: 0.0,
            height: 3.0,
        }),
    );
    let lid = l.id;
    doc.commit(Transaction::new("level").insert(l)).unwrap();
    let water = Element::new(
        "Site water",
        ElementKind::SiteFeature(SiteFeature {
            layer: "bgt_waterdeel".into(),
            category: SiteCategory::Water,
            features: vec![SiteRow {
                key: "1".into(),
                geom: SiteGeometry::Polygon {
                    exterior: vec![
                        DVec2::new(0.0, 0.0),
                        DVec2::new(4.0, 0.0),
                        DVec2::new(4.0, 3.0),
                        DVec2::new(0.0, 3.0),
                    ],
                    holes: vec![],
                },
            }],
        }),
    );
    doc.commit(Transaction::new("site").insert(water)).unwrap();
    let bound = Element::new(
        "Site grens",
        ElementKind::SiteFeature(SiteFeature {
            layer: "kadastrale_grens".into(),
            category: SiteCategory::Boundary,
            features: vec![SiteRow {
                key: "2".into(),
                geom: SiteGeometry::Polyline {
                    points: vec![DVec2::new(0.0, 0.0), DVec2::new(5.0, 0.0)],
                },
            }],
        }),
    );
    doc.commit(Transaction::new("site").insert(bound)).unwrap();
    (doc, lid)
}

#[test]
fn site_polygons_fill_and_outline_on_every_plan() {
    let (doc, lid) = site_model();
    let plan = plan_view(doc.model(), lid, 1.2);
    let fills: Vec<_> = plan
        .items
        .iter()
        .filter_map(|it| match it {
            PlanItem::Fill { hatch, points, .. } => Some((*hatch, points.len())),
            _ => None,
        })
        .collect();
    // Water fills with its Cross hatch (the category drives the pattern).
    assert!(fills.iter().any(|(h, n)| *h == Hatch::Cross && *n == 4));
    let outlines: Vec<_> = plan
        .items
        .iter()
        .filter_map(|it| match it {
            PlanItem::Polygon { pen, points, .. } if *pen == Pen::Projection => Some(points.len()),
            _ => None,
        })
        .collect();
    assert!(
        outlines.contains(&4),
        "water outline drawn, has {outlines:?}"
    );
    // The boundary polyline draws as one light segment.
    let lines: Vec<_> = plan
        .items
        .iter()
        .filter(|it| {
            matches!(
                it,
                PlanItem::Line {
                    pen: Pen::Projection,
                    ..
                }
            )
        })
        .collect();
    assert_eq!(lines.len(), 1);
}

#[test]
fn site_layers_mesh_flat_and_ghosted_for_3d() {
    use creabim_geom::scene::build_scene;
    let (doc, _) = site_model();
    let scene = build_scene(doc.model());
    let items: Vec<_> = scene
        .iter()
        .filter(|i| {
            doc.model()
                .get(i.id)
                .is_some_and(|e| matches!(e.kind, ElementKind::SiteFeature(_)))
        })
        .collect();
    // Two site elements (water polygon + boundary polyline); only the
    // polygon meshes — polylines stay plan-only for now.
    assert_eq!(items.len(), 1);
    let item = items[0];
    assert!(item.ghost, "underlay, not a body");
    assert!(!item.mesh.is_empty());
    // Flat at project zero, inside the square, facing up.
    for p in &item.mesh.positions {
        assert_eq!(p[2], 0.0);
        assert!(p[0] >= 0.0 && p[0] <= 4.0 && p[1] >= 0.0 && p[1] <= 3.0);
    }
    assert!(item.mesh.normals.iter().all(|n| (n[2] - 1.0).abs() < 1e-6));
}
