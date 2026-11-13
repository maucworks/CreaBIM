# CreaBIM quality-of-life shortcuts. `make help` lists them.
#
# Profiles: `release` is fully optimised (slow to build); `fast` is the
# daily driver (thin LTO, debug symbols, quick to link). Plain `cargo`
# commands work too — these just save typing.
#
# Embedded Python: scripting links a pinned python-build-standalone
# bundle (never a system Python). The targets below provision it first
# and point PyO3 at it; `make env` shows the resolved paths. The pin
# itself lives in creabim-provision (PINNED_PYTHON/PINNED_TAG); the
# directory name here must match it.

PROFILE ?= fast

# Where the bundle lives. Honors an exported CREABIM_PYTHON_HOME,
# otherwise the platform temp dir (same default as creabim-provision).
TMP ?= $(or $(TMPDIR),/tmp)
CACHE_ROOT = $(or $(CREABIM_PYTHON_HOME),$(TMP)/creabim-python-3.13.18-20261110)
PY_HOME := $(CACHE_ROOT)/python

ifeq ($(OS),Windows_NT)
PY_EXE := $(PY_HOME)/python.exe
export PATH := $(PY_HOME):$(PY_HOME)/DLLs:$(PATH)
else
PY_EXE := $(PY_HOME)/bin/python3.13
UNAME := $(shell uname -s)
ifeq ($(UNAME),Darwin)
export DYLD_LIBRARY_PATH := $(PY_HOME)/lib
else
export LD_LIBRARY_PATH := $(PY_HOME)/lib
endif
endif
export PYO3_PYTHON := $(PY_EXE)

# Bake the bundle's lib dir into dev binaries so they (and the test
# binaries) find libpython without DYLD/LD_LIBRARY_PATH at runtime.
# Absolute path, so it covers this machine's build only; CI and
# releases don't use these flags.
ifneq ($(OS),Windows_NT)
export RUSTFLAGS := $(RUSTFLAGS) -C link-args=-Wl,-rpath,$(PY_HOME)/lib
endif

.PHONY: help setup env build rebuild run run-release cli test check clippy fmt fmt-check lint bless validate-ifc python docs clean distclean

help: ## list these shortcuts
	@grep -E '^[a-z-]+:.*?## ' $(MAKEFILE_LIST) | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "  %-12s %s\n", $$1, $$2}'

setup: ## fetch + verify the pinned CPython bundle (cached afterwards)
	CREABIM_PYTHON_HOME="$(CACHE_ROOT)" cargo run -q -p creabim-provision

env: ## show the Python bundle paths the targets below use
	@echo "CACHE_ROOT=$(CACHE_ROOT)"
	@echo "PYO3_PYTHON=$(PYO3_PYTHON)"
	@echo "loader path: $(or $(DYLD_LIBRARY_PATH),$(LD_LIBRARY_PATH),see PATH on Windows)"

build: setup ## build the workspace (fast profile; PROFILE=release for full optimisations)
	cargo build --profile $(PROFILE)

rebuild: setup ## force relink against the bundled Python, then build (fixes stale libpython links)
	cargo clean -p creabim-script -p creabim-provision -p pyo3 -p pyo3-ffi -p pyo3-macros
	cargo build --profile $(PROFILE)

run: setup ## run the app (fast profile; PROFILE=release for full optimisations)
	cargo run --profile $(PROFILE) -p creabim-app

run-release: ## run the app, fully optimised
	$(MAKE) run PROFILE=release

cli: setup ## build the headless CLI
	cargo build --profile $(PROFILE) -p creabim-cli

test: setup ## the whole test suite
	cargo test --workspace

check: setup ## fast typecheck of the whole workspace
	cargo check --workspace

clippy: setup ## lints, denied warnings like CI
	cargo clippy --workspace --all-targets -- -D warnings

fmt: ## format everything in place
	cargo fmt --all

fmt-check: ## fail when anything needs formatting, like CI
	cargo fmt --all --check

lint: fmt-check clippy ## CI's static gates, local

docs: ## build the creabim builder API reference locally (needs pip + pdoc)
	python3 -m pip install --quiet "pdoc>=15,<17"
	PYTHONPATH=crates/creabim-script/python \
		python3 -m pdoc --docformat google -o docs/_build creabim
	@echo "open docs/_build/index.html; the guide is docs/scripting-guide.md"

bless: ## regenerate docs/json-format.md and the JSON schema from the format
	CREABIM_BLESS=1 cargo test -p creabim-spec

validate-ifc: cli ## sample houses round-trip through IFC and validate (needs python + ifcopenshell)
	@for suffix in "" -simple -layered -typed; do \
		name=house$$suffix; \
		target/$(PROFILE)/creabim sample$$suffix $$name.creabim.json; \
		target/$(PROFILE)/creabim ifc $$name.creabim.json $$name.ifc; \
		python3 tools/validate_ifc.py $$name.ifc; \
	done

python: setup ## open a REPL in the bundled interpreter
	"$(PY_EXE)"

clean: ## drop build artefacts
	cargo clean

distclean: clean ## drop build artefacts and the provisioned Python bundle
	rm -rf "$(CACHE_ROOT)"
