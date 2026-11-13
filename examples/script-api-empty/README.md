# Script API demo: a cabin from an empty project

`script run` commits into whichever project it is run against. This
demo proves it from almost nothing: the model starts as one level
plus units (built from `empty-spec.json`), and a single script block
adds everything else — walls sampled from a filleted geometrykit
`Path` footprint (straight runs as straight walls, fillet arcs as
true arc walls, no segmentation), a door, two windows, a vent hole
and a mesh pyramid roof.

Open `demo.creabim` in the app: four walls closing a 6×4 m room with
a pyramid roof over it, all native elements (plan views, schedules,
IFC). The empty spec ships no views, so add one from the Create
menu: **3D view** (drag it to orbit), then place it on a sheet with
Place view….

The CLI below is `target/debug/creabim` from this directory
(`make cli` builds it; `make run` starts the app).

## 1. Rebuild the demo from scratch

```sh
BIN=target/debug/creabim
DEMO=examples/script-api-empty
$BIN build $DEMO/empty-spec.json $DEMO/demo.creabim.json
# → 2 elements: 1 Level, 1 Units
$BIN script add $DEMO/demo.creabim.json Cabin $DEMO/cabin.py --require geometrykit
$BIN script run $DEMO/demo.creabim.json Cabin --out $DEMO/demo.creabim.json
# → script Cabin: 9 added, 1 replaced, 0 removed
$BIN save-db $DEMO/demo.creabim.json $DEMO/demo.creabim   # for the app
```

`script add` adopts `cabin.py` into `scripts/cabin.py` and registers
it (`script registry list` shows `Cabin -> scripts/cabin.py`).
`--require geometrykit` installs the math backend the footprint
samples with (pip, once, into the app-managed site-packages).

## 2. Run headless

```sh
$BIN script run $DEMO/demo.creabim.json Cabin --out $DEMO/demo.creabim.json
# → script Cabin: 0 added, 10 replaced, 0 removed (id-stable re-run)
$BIN schedule $DEMO/demo.creabim.json doors
# → D-C1,Ground Floor,Cabin wall-1,900,2100,0,"Right, in"
$BIN schedule $DEMO/demo.creabim.json windows
# → W-C1,Ground Floor,Cabin wall-3,1200,1300,900
# → W-C2,Ground Floor,Cabin wall-7,1200,1300,900
```

## Note: `# in` comments

The front-matter parser reads any `# in ...` comment line as an
input declaration — a comment like `# in the north wall` fails
validation (`expected '# in: name: kind = default'`). Phrase such
comments so they don't open with `# in`.
