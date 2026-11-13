# Scripts

A **script** is a bespoke object harbouring a program: declare its inputs
in the script header, run it, and it builds or edits ordinary elements.

```python
# in: bays: int = 4
# in: depth: float = 3.0
```

Scripts run from the command line for now
(`creabim script run model file --element name`): each run commits one
undoable step, and re-running replaces the script's own outputs by key,
so dimensions and tags pointing at them survive.
