# Scripting guide

Script blocks generate native CreaBIM elements from Python. This guide
covers the whole loop: declaring inputs, emitting outputs, linking the
block to a file on disc, and how re-runs keep element identity. The
function reference for the builder package lives on the API site
(python API, `creabim` module); what follows is the protocol the
reference assumes.

## The protocol in brief

A script block reads two variables and writes one list:

- `INPUTS`: a dict of the block's declared inputs (see below).
- `OUTPUTS`: a list of op dicts, each `{"key": ..., <one op>: {...}}`.
  The host validates every op and materializes it into a native
  element — walls, meshes, openings, doors, windows — inside one
  undoable transaction labelled `script:<name>`.

`key` is the stable identity. Re-running the script replaces the
element stored under a known key (dimensions, tags and downstream
references survive the regen) and removes elements whose keys
vanished, with cascade. A missing `OUTPUTS` is a valid empty run.

```python
from creabim import wall, door

n = INPUTS["bays"]
d = INPUTS["depth"]
OUTPUTS = [
    wall(f"w-{i}", "Ground Floor", [i * d / n, 0.0], [i * d / n, 4.0])
    for i in range(n + 1)
] + [
    door("d-0", "w-1", offset=1.0, mark="D02"),
]
```

Use the builders (`from creabim import wall, door, ...`) instead of
hand-writing dicts: they carry the signatures and emit exactly what
the host parser accepts.

## Declaring inputs

The block's front-matter header declares typed inputs, one per line —
the single source of truth (Grasshopper-solver style). The app builds
the parameter widgets from it:

```
# in: bays: int = 3
# in: depth: float = 4.0
# in: host: element
```

Element inputs name a model element and give the script a live handle
to it; any model change re-runs a script that has one. Bad headers
are element validation errors: they fail at commit time, not at run
time.

## Op vocabulary

One op kind per dict; anything else is an error naming the op key.
Field sets mirror the matching ifckit `Pending*` where one exists,
so authors learn one vocabulary:

- `wall`: `level` (element name, required), `start`/`end` (`[x, y]`,
  required), `thickness` (default 0.1), `height` (default: the level
  height), `name`, `mark`. `walls()` builds a run through a
  polyline — the Path-to-walls conversion: sample a geometrykit
  `Path` in the script and pass the points; segments share
  endpoints so the join solver corners them cleanly. `arc_wall()`
  takes an arc's centre, start and sweep and bends one wall along
  it — fillet arcs stay arcs; see the cabin demo.
- `mesh`: `verts` (`[[x, y, z]]`, required), `faces` (required),
  `name`.
- `opening`: a bare hole. `host` (required), `width`/`height`
  (required), `offset`/`sill` (default 0).
- `door`: `host` (required), `offset` (0), `width` (0.9), `height`
  (2.1), `sill` (0), `flip_hand`, `flip_facing`, `name`, `mark`.
- `window`: `host` (required), `offset` (0), `width` (1.2), `height`
  (1.3), `sill` (0.9), `panes_across`/`panes_up` (1), `frame_width` /
  `frame_depth` (fitted to the opening and the wall when omitted),
  `flip_facing`, `name`, `mark`.

Hosted ops (`opening`/`door`/`window`) name their host wall by
element name — or by the key of a wall op in the *same* `OUTPUTS`
list, so a script can cut a hole in a wall it builds in the same
run. Same-run keys win over element names; the host must be a wall.
`mark` feeds the door/window schedules.

## Disc sources and the registry

A block's source of truth can live on disc: the project keeps a
**script registry** (one per model) mapping entry names onto files
under the project's `scripts/` directory, and each block names its
entry. The linked file outranks the embedded copy; the embedded copy
is the offline fallback, so a shared model without its files still
runs.

- `script add <model> <name> <file.py>` adopts the file into
  `scripts/` and registers it.
- `script link <model> <name> [--file <path>] [--as <entry>]`
  registers the block's entrance point, adopts a file, or attaches
  to an existing entry (sharing its module).
- `script export` writes the embedded copy out; `script embed`
  drops the link (pruning the entry when unused).
- `script registry list|remove` manages entries directly.

The app reads linked files before every recompute pass and writes
panel edits straight back to the file. The recompute key covers
every `*.py` under `scripts/`, so a changed helper module re-runs
every script that could import it. Several blocks may share one
entry; modules calling into each other is later work.

## Route B (the one rule)

Scripts emit vocabulary, never elements: builders return dicts, the
host validates and materializes. No commit access, no mutation of
foreign elements, no model writes from user code — runs are pure
functions of snapshot plus inputs. That is what keeps undo, journal
replay, drawings, sheets, schedules and IFC export working without
knowing scripts exist.
