"""Typed builders for CreaBIM script outputs (M-S7b).

A script block speaks to the host in op dicts: ``OUTPUTS`` is a list of
``{"key": ..., <one op kind>: {...}}`` and the host validates and
materializes each op into a native element (Route B — builders emit
vocabulary, never elements). These constructors build those dicts, so
authors get signatures and docstrings instead of raw dicts, and the
reference below doubles as the op specification: every builder emits
exactly what the host parser accepts.

Protocol in brief: ``key`` is the stable identity — re-running the
script replaces the element stored under a known key and removes
elements whose keys vanished (cascade). Hosted ops (``opening``,
``door``, ``window``) name their host wall by element name, or by the
key of a wall op in the same ``OUTPUTS`` list (same-run keys win).
A script never mutates the model and never calls undo: the host maps
ops onto one ``Transaction`` per run.

Disc discipline (M-S6): the project's ``scripts/`` directory is on
``sys.path`` for the run, so ``from creabim import door`` finds this
package and ``import helpers`` finds the project's own modules.
"""

__all__ = ["wall", "arc_wall", "walls", "mesh", "opening", "door", "window"]


def _op(key, kind, name=None, mark=None, **fields):
    """One keyed op dict: ``{"key": key, kind: fields}``."""
    op = {"key": key, kind: dict(fields)}
    if name is not None:
        op[kind]["name"] = name
    if mark is not None:
        op[kind]["mark"] = mark
    return op


def wall(key, level, start, end, thickness=0.1, height=None, bulge=0.0, name=None,
         mark=None):
    """A wall on a level, straight or arched.

    Args:
        key: Stable output key; re-runs replace the stored wall.
        level: Name of the level element (e.g. ``"Ground Floor"``).
        start: ``[x, y]`` wall start in millimetres.
        end: ``[x, y]`` wall end in millimetres.
        thickness: Wall thickness in millimetres.
        height: Wall height in millimetres; ``None`` takes the level height.
        bulge: Centreline bend, 0 draws straight. Otherwise the arc
            through the chord ``start`` → ``end`` with sweep
            ``4*atan(bulge)``: positive bends right of ``start`` →
            ``end`` (counter-clockwise). A quarter circle is
            ``±tan(pi/8)``; a semicircle ``±1``. Full circles cannot be
            one wall — split them, or use :func:`arc_wall` twice.
        name: Element name; defaults to ``"wall {key}"``.
        mark: Element mark (shows in tags and schedules).

    Returns:
        An op dict for ``OUTPUTS``.
    """
    return _op(
        key,
        "wall",
        name=name,
        mark=mark,
        level=level,
        start=list(start),
        end=list(end),
        thickness=thickness,
        height=height,
        bulge=bulge,
    )


def arc_wall(key, level, center, start, angle, thickness=0.1, height=None, name=None,
             mark=None):
    """An arched wall from a centre, a start point and a sweep angle.

    This is the Arc-to-wall conversion: pass a geometrykit ``Arc``'s
    ``center``, ``start`` and ``angle`` straight through (in the plan
    plane) and the wall follows the same arc — no segmentation. The
    sweep is counter-clockwise positive, matching the arc; ``end`` is
    derived, so the chord always agrees with the angle.

    Args:
        key: Stable output key; re-runs replace the stored wall.
        level: Name of the level element.
        center: ``[x, y]`` arc centre in millimetres.
        start: ``[x, y]`` arc start in millimetres.
        angle: Sweep in radians, counter-clockwise positive. Must be
            non-zero and under a full circle — split circles into two
            semicircular walls.
        thickness: Wall thickness in millimetres.
        height: Wall height in millimetres; ``None`` takes the level
            height.
        name: Element name; defaults to ``"wall {key}"``.
        mark: Element mark.

    Returns:
        An op dict for ``OUTPUTS``.
    """
    import math

    if not (0.0 < abs(angle) < 2.0 * math.pi):
        raise ValueError(
            "arc_wall() needs a non-zero sweep under a full circle; "
            "split circles into two semicircles"
        )
    cx, cy = center
    sx, sy = start
    co, si = math.cos(angle), math.sin(angle)
    end = [cx + (sx - cx) * co - (sy - cy) * si, cy + (sx - cx) * si + (sy - cy) * co]
    return _op(
        key,
        "wall",
        name=name,
        mark=mark,
        level=level,
        start=list(start),
        end=end,
        thickness=thickness,
        height=height,
        bulge=math.tan(angle / 4.0),
    )


def walls(key, level, points, thickness=0.1, height=None, name=None, mark=None):
    """A wall run through a polyline: one wall op per segment.

    This is the Path-to-walls conversion: sample a geometrykit
    ``Path`` in the script (``path.sample()``, ``divide()``,
    ``point_at()``) and pass the points here. Consecutive segments
    share their endpoints exactly, so the host's join solver corners
    them cleanly with no extra elements.

    Args:
        key: Key stem; segment ``i`` is stored under ``f"{key}-{i}"``,
            so re-runs replace segment for segment.
        level: Name of the level element.
        points: List of ``[x, y]`` in millimetres, at least two. The
            script typically samples these from a geometrykit ``Path``;
            this builder takes plain points and stays free of that
            dependency.
        thickness: Wall thickness in millimetres.
        height: Wall height in millimetres; ``None`` takes the level
            height.
        name: Element name stem; defaults to ``"wall {key}"`` with a
            per-segment suffix.
        mark: Element mark, carried onto every segment.

    Returns:
        A list of op dicts for ``OUTPUTS`` (splice it in with ``+``).
    """
    pts = [list(p) for p in points]
    if len(pts) < 2:
        raise ValueError("walls() needs at least two points")
    stem = name if name is not None else f"wall {key}"
    return [
        _op(
            f"{key}-{i}",
            "wall",
            name=f"{stem}-{i}",
            mark=mark,
            level=level,
            start=pts[i],
            end=pts[i + 1],
            thickness=thickness,
            height=height,
        )
        for i in range(len(pts) - 1)
    ]


def mesh(key, verts, faces, name=None):
    """A free mesh (mass, terrain patch, custom solid).

    Args:
        key: Stable output key; re-runs replace the stored mesh.
        verts: List of ``[x, y, z]`` vertices in millimetres.
        faces: List of faces, each a list of vertex indices.
        name: Element name; defaults to ``"mesh {key}"``.

    Returns:
        An op dict for ``OUTPUTS``.
    """
    return _op(
        key, "mesh", name=name, verts=[list(v) for v in verts], faces=[list(f) for f in faces]
    )


def opening(key, host, width, height, offset=0.0, sill=0.0, name=None, mark=None):
    """A bare rectangular hole cut into a wall (no door or window in it).

    Args:
        key: Stable output key; re-runs replace the stored opening.
        host: Host wall — an element name, or the key of a wall op in
            the same ``OUTPUTS`` list (same-run keys win).
        width: Hole width in millimetres.
        height: Hole height in millimetres.
        offset: Distance from the wall start to the hole's left edge.
        sill: Hole bottom above the wall base.
        name: Element name; defaults to ``"opening {key}"``.
        mark: Element mark.

    Returns:
        An op dict for ``OUTPUTS``.
    """
    return _op(
        key,
        "opening",
        name=name,
        mark=mark,
        host=host,
        width=width,
        height=height,
        offset=offset,
        sill=sill,
    )


def door(key, host, offset=0.0, width=0.9, height=2.1, sill=0.0, flip_hand=False,
         flip_facing=False, name=None, mark=None):
    """A door cut into its host wall.

    Args:
        key: Stable output key; re-runs replace the stored door.
        host: Host wall — an element name, or the key of a wall op in
            the same ``OUTPUTS`` list (same-run keys win).
        offset: Distance from the wall start to the door's left edge.
        width: Door width in millimetres.
        height: Door height in millimetres.
        sill: Door bottom above the wall base (0 for a level floor).
        flip_hand: Hinge the door on its other jamb.
        flip_facing: Open the door to the wall's exterior.
        name: Element name; defaults to ``"door {key}"``.
        mark: Element mark (shows in the door schedule).

    Returns:
        An op dict for ``OUTPUTS``.
    """
    return _op(
        key,
        "door",
        name=name,
        mark=mark,
        host=host,
        offset=offset,
        width=width,
        height=height,
        sill=sill,
        flip_hand=flip_hand,
        flip_facing=flip_facing,
    )


def window(key, host, offset=0.0, width=1.2, height=1.3, sill=0.9, panes_across=1,
           panes_up=1, frame_width=0.06, frame_depth=0.1, flip_facing=False, name=None,
           mark=None):
    """A window cut into its host wall, with frame and glazing bars.

    Args:
        key: Stable output key; re-runs replace the stored window.
        host: Host wall — an element name, or the key of a wall op in
            the same ``OUTPUTS`` list (same-run keys win).
        offset: Distance from the wall start to the window's left edge.
        width: Window width in millimetres.
        height: Window height in millimetres.
        sill: Window bottom above the wall base.
        panes_across: Panes side by side, split by glazing bars.
        panes_up: Panes stacked, split by glazing bars.
        frame_width: Face width of the frame members.
        frame_depth: Depth of the frame through the wall (clamped to
            what fits the opening and the wall thickness).
        flip_facing: Face the window's outside (and sill) to the wall's
            interior.
        name: Element name; defaults to ``"window {key}"``.
        mark: Element mark (shows in the window schedule).

    Returns:
        An op dict for ``OUTPUTS``.
    """
    return _op(
        key,
        "window",
        name=name,
        mark=mark,
        host=host,
        offset=offset,
        width=width,
        height=height,
        sill=sill,
        panes_across=panes_across,
        panes_up=panes_up,
        frame_width=frame_width,
        frame_depth=frame_depth,
        flip_facing=flip_facing,
    )
