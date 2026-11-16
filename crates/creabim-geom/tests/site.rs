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
    // one homogenous element for the file, two layers inside
    let file = Element::new(
        "Site site",
        ElementKind::SiteFeature(SiteFeature::file(vec![
            SiteLayer {
                layer: "bgt_waterdeel".into(),
                category: SiteCategory::Water,
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
                        holes: vec![],
                    },
                }],
            },
            SiteLayer {
                layer: "kadastrale_grens".into(),
                category: SiteCategory::Boundary,
                style: LayerStyle::default(),
                features: vec![SiteRow {
                    key: "2".into(),
                    geom: SiteGeometry::Polyline {
                        points: vec![DVec2::new(0.0, 0.0), DVec2::new(5.0, 0.0)],
                    },
                }],
            },
        ])),
    );
    doc.commit(Transaction::new("site").insert(file)).unwrap();
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
            PlanItem::Polygon {
                pen: Pen::SiteLayer { .. },
                points,
                ..
            } => Some(points.len()),
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
                    pen: Pen::SiteLayer { .. },
                    ..
                }
            )
        })
        .collect();
    assert_eq!(lines.len(), 1);
}

#[test]
fn a_hidden_layer_draws_nothing_and_a_style_colours_the_fill() {
    use creabim_core::styles::Rgb;
    let (mut doc, lid) = site_model();
    let file = doc
        .model()
        .iter()
        .find(|e| matches!(e.kind, ElementKind::SiteFeature(_)))
        .unwrap()
        .clone();
    // the water layer gets a fill colour under its hatch; the boundary
    // layer hides project-wide — both inside the one file element
    let mut styled = file.clone();
    if let ElementKind::SiteFeature(s) = &mut styled.kind {
        for l in s.layers_mut() {
            if l.layer == "bgt_waterdeel" {
                l.style.fill = Some(Rgb([0.2, 0.4, 0.6]));
            }
            if l.layer == "kadastrale_grens" {
                l.style.visible = false;
            }
        }
    }
    doc.commit(Transaction::new("site style test").replace(file, styled))
        .unwrap();
    let plan = plan_view(doc.model(), lid, 1.2);
    let fills: Vec<_> = plan
        .items
        .iter()
        .filter_map(|it| match it {
            PlanItem::Fill { hatch, rgb, .. } => Some((*hatch, *rgb)),
            _ => None,
        })
        .collect();
    assert_eq!(fills.len(), 1, "only the water still fills, {fills:?}");
    assert!(fills[0].1.is_some_and(|c| (c[0] - 0.2).abs() < 1e-6));
    assert!(
        !plan.items.iter().any(|it| matches!(
            it,
            PlanItem::Line {
                pen: Pen::SiteLayer { .. },
                ..
            }
        )),
        "the hidden boundary draws no polyline"
    );
    // the water outline draws in the layer's own pen
    assert!(
        plan.items.iter().any(|it| matches!(
            it,
            PlanItem::Polygon {
                pen: Pen::SiteLayer { .. },
                ..
            }
        )),
        "the water outline draws in the layer pen"
    );
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
