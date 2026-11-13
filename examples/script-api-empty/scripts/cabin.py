"""A whole cabin from an empty project: walls sampled from a
geometrykit Path footprint — straight runs as straight walls, fillet
arcs as arc walls — plus a door, two windows, a vent hole and a mesh
pyramid roof.
"""

import warnings

warnings.filterwarnings("ignore", category=DeprecationWarning)

from geometrykit import Arc, Path, Vec

from creabim import arc_wall, door, mesh, opening, wall, window

LVL = "Ground Floor"

footprint = Path.from_pts(
    [Vec(0, 0, 0), Vec(6, 0, 0), Vec(6, 4, 0), Vec(0, 4, 0)], closed=True
)
footprint.fillet([0, 1, 2, 3], 0.5)

# Straight runs stay straight walls; fillet arcs become arc walls with
# the same sweep — no segmentation. Filleting puts the arcs at the
# even indices, so the odd indices run south, east, north, west.
runs = []
for i, seg in enumerate(footprint.segments):
    p0 = [seg.start.x, seg.start.y]
    p1 = [seg.end.x, seg.end.y]
    if isinstance(seg, Arc):
        runs.append(
            arc_wall(
                f"c-wall-{i}",
                LVL,
                [seg.center.x, seg.center.y],
                p0,
                seg.angle,
                thickness=0.2,
                name=f"Cabin wall-{i}",
            )
        )
    else:
        runs.append(
            wall(
                f"c-wall-{i}",
                LVL,
                p0,
                p1,
                thickness=0.2,
                name=f"Cabin wall-{i}",
            )
        )

OUTPUTS = runs + [
    # Hosts are same-run wall keys; arcs host too (offset runs round them).
    door("c-door", "c-wall-1", offset=1.0, mark="D-C1"),
    window("c-win-e", "c-wall-3", offset=1.0, mark="W-C1"),
    window("c-win-w", "c-wall-7", offset=1.0, mark="W-C2"),
    opening("c-vent", "c-wall-5", width=0.5, height=0.5, sill=1.9),
    # Pyramid roof: one mesh, five vertices, four faces.
    mesh(
        "c-roof",
        verts=[
            [-0.3, -0.3, 2.7],
            [6.3, -0.3, 2.7],
            [6.3, 4.3, 2.7],
            [-0.3, 4.3, 2.7],
            [3.0, 2.0, 4.2],
        ],
        faces=[[0, 1, 4], [1, 2, 4], [2, 3, 4], [3, 0, 4]],
        name="Cabin roof",
    ),
]
