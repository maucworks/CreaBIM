# CreaBIM quality-of-life shortcuts. `make help` lists them.
#
# Profiles: `release` is fully optimised (slow to build); `fast` is the
# daily driver (thin LTO, debug symbols, quick to link). Plain `cargo`
# commands work too — these just save typing.

PROFILE ?= fast

.PHONY: help run run-release cli test check clippy fmt fmt-check lint bless validate-ifc clean

help: ## list these shortcuts
	@grep -E '^[a-z-]+:.*?## ' $(MAKEFILE_LIST) | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "  %-12s %s\n", $$1, $$2}'

run: ## run the app (fast profile; PROFILE=release for full optimisations)
	cargo run --profile $(PROFILE) -p creabim-app

run-release: ## run the app, fully optimised
	$(MAKE) run PROFILE=release

cli: ## build the headless CLI
	cargo build --profile $(PROFILE) -p creabim-cli

test: ## the whole test suite
	cargo test --workspace

check: ## fast typecheck of the whole workspace
	cargo check --workspace

clippy: ## lints, denied warnings like CI
	cargo clippy --workspace --all-targets -- -D warnings

fmt: ## format everything in place
	cargo fmt --all

fmt-check: ## fail when anything needs formatting, like CI
	cargo fmt --all --check

lint: fmt-check clippy ## CI's static gates, local

bless: ## regenerate docs/json-format.md and the JSON schema from the format
	CREABIM_BLESS=1 cargo test -p creabim-spec

validate-ifc: cli ## sample houses round-trip through IFC and validate (needs python + ifcopenshell)
	@for suffix in "" -simple -layered -typed; do \
		name=house$$suffix; \
		target/$(PROFILE)/creabim sample$$suffix $$name.creabim.json; \
		target/$(PROFILE)/creabim ifc $$name.creabim.json $$name.ifc; \
		python3 tools/validate_ifc.py $$name.ifc; \
	done

clean: ## drop build artefacts
	cargo clean
