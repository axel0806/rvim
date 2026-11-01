# Thin wrappers over cargo. Works with POSIX make, GNU make, and make in Git Bash.
# Run `make help` for the list of targets.

CARGO ?= cargo
ARGS ?=

.PHONY: help build debug test test-vim lint fmt check run install clean

help: ## List the targets
	@grep -E '^[a-zA-Z_-]+:.*## ' Makefile | sed -e 's/:[^#]*## /	/' | awk -F'	' '{ printf "  %-10s %s\n", $$1, $$2 }'

build: ## Release build (target/release/rvim)
	$(CARGO) build --release

debug: ## Debug build (target/debug/rvim)
	$(CARGO) build

test: ## Run the tests (differential tests skip without Vim)
	$(CARGO) test

test-vim: ## Run the tests and fail if Vim 9 is missing
	RVIM_REQUIRE_VIM=1 $(CARGO) test

lint: ## Clippy with warnings denied, and fmt --check
	$(CARGO) clippy --all-targets -- -D warnings && $(CARGO) fmt --check

fmt: ## Format the code
	$(CARGO) fmt

check: lint test-vim ## lint, then test-vim (run before a PR)

run: ## Run rvim; pass arguments as ARGS=file.txt
	$(CARGO) run -- $(ARGS)

install: ## Install rvim with cargo install --locked
	$(CARGO) install --path . --locked

clean: ## Remove build output (target/)
	$(CARGO) clean
