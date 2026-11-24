# CreaBIM MCP server

A local MCP server for saved CreaBIM projects and model specifications. It runs over stdio, using the official Python MCP SDK and the existing CreaBIM CLI. No network service or account is needed after setup.

## Setup

From the repository root, build the CLI and install the Python dependency in a virtual environment:

```sh
cargo build --locked -p creabim-cli
python3 -m venv tools/mcp/.venv
tools/mcp/.venv/bin/python -m pip install -r tools/mcp/requirements.txt
```

Python 3.10 or newer is required. On Windows use `tools\mcp\.venv\Scripts\python.exe` instead. The server looks for `creabim` on PATH, then in the repository's `target/release` and `target/debug` folders. To select a particular binary, set `CREABIM_CLI` to its full path. Use a CLI built from a compatible CreaBIM version.

In an MCP host that accepts `mcpServers` JSON, add this entry, replacing both paths with absolute paths on your machine:

```json
{
  "mcpServers": {
    "creabim": {
      "command": "/absolute/path/to/CreaBIM/tools/mcp/.venv/bin/python",
      "args": ["/absolute/path/to/CreaBIM/tools/mcp/server.py"],
      "env": {"CREABIM_CLI": "/absolute/path/to/CreaBIM/target/debug/creabim"}
    }
  }
}
```

The host launches the server; it does not print a URL. For a first check, ask the client to validate `examples/small-house.json`, build it to a **new** `.creabim` file, list its `Wall` elements, and retrieve one by ID. The schema tool describes the supported JSON specification.

## Tools

- `inspect_project(project_path)`: saved project metadata and element count.
- `list_elements(project_path, kind, offset, limit, search)`: paginated IDs, kinds and names. Filters are optional; `kind` is exact, and `search` matches part of a name. SQLite ignores case for ASCII letters only.
- `get_element(project_path, element_id)`: complete stored JSON properties for one element; lengths follow the model's units (normally metres).
- `get_schema()`: JSON schema for a model specification.
- `check_spec(spec_path)`: validate a specification and return any CLI warnings.
- `build_project(spec_path, output_path)`: build a new `.creabim` file and return any warnings. It refuses existing files and SQLite sidecars; it does not overwrite them.

The server reads format-version 1 `.creabim` databases from disk in SQLite read-only mode. It does not support legacy `.creabim.json` for inspection or control the open CreaBIM window, and it cannot see changes that have not been saved. It depends on CreaBIM's current SQLite schema. Use a new output path that no other process is creating or opening at the same time; sidecar checks and creation are not one atomic operation. Creating a project requires hard-link support in the destination filesystem; some removable or network volumes do not support it. The host controls access to local paths: only connect trusted clients and review paths before invoking tools.

## Test

```sh
tools/mcp/.venv/bin/python -m unittest discover -s tools/mcp -p 'test_*.py' -v
```

The protocol test launches the server with the official MCP client and uses `target/debug/creabim`, so run `cargo build --locked -p creabim-cli` first. It validates and builds `examples/small-house.json` in a temporary directory and checks read-only inspection and overwrite protection.
