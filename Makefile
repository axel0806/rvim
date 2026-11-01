# Thin wrappers over cargo, plus install, uninstall and dist for packagers.
# Works with POSIX make, GNU make, and make in Git Bash.
# Run `make help` for the list of targets.
#
# Packagers (Unix; see INSTALL.md, "Packaging"):
#   make install DESTDIR="$pkgdir" PREFIX=/usr
#   make install SKIP_BUILD=1 DESTDIR=... PREFIX=...   (binary already built)
#   make uninstall DESTDIR=... PREFIX=...
#   make dist                     (source archive from git; no tag, no upload)
# install and uninstall are for Unix packagers: they use install(1), mkdir -p
# and rmdir. Set BIN if cargo wrote the binary elsewhere (CARGO_TARGET_DIR).

CARGO ?= cargo
ARGS ?=

# Install locations. DESTDIR is empty by default; packagers set it to a staging
# directory. Both are honoured by install and uninstall only.
PREFIX ?= /usr/local
DESTDIR ?=
# SKIP_BUILD=1: install does not run `make build` first.
SKIP_BUILD ?=
# The release binary that install copies.
BIN ?= target/release/rvim

BINDIR = $(DESTDIR)$(PREFIX)/bin
LICDIR = $(DESTDIR)$(PREFIX)/share/licenses/rvim
DOCDIR = $(DESTDIR)$(PREFIX)/share/doc/rvim

.PHONY: help build debug test test-vim lint fmt check run install uninstall dist cargo-install clean

help: ## List the targets
	@grep -E '^[a-zA-Z_-]+:.*## ' Makefile | sed -e 's/:[^#]*## /	/' | awk -F'	' '{ printf "  %-14s %s\n", $$1, $$2 }'

build: ## Release build with the lock file (target/release/rvim)
	$(CARGO) build --release --locked

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

install: ## Install rvim, licences and README under PREFIX (DESTDIR staging; builds first unless SKIP_BUILD=1)
	@if [ -z "$(SKIP_BUILD)" ]; then $(MAKE) build; fi
	@test -f "$(BIN)" || { echo "install: $(BIN) not found; run make build or set BIN" >&2; exit 1; }
	mkdir -p "$(BINDIR)" "$(LICDIR)" "$(DOCDIR)"
	install -m 755 "$(BIN)" "$(BINDIR)/rvim"
	install -m 644 LICENSE LICENSE-VIM NOTICE "$(LICDIR)/"
	install -m 644 README.md "$(DOCDIR)/README.md"

uninstall: ## Remove the files install put under PREFIX; rmdir empty directories only
	rm -f "$(BINDIR)/rvim" "$(LICDIR)/LICENSE" "$(LICDIR)/LICENSE-VIM" "$(LICDIR)/NOTICE" "$(DOCDIR)/README.md"
	rmdir "$(LICDIR)" 2>/dev/null || true
	rmdir "$(DOCDIR)" 2>/dev/null || true
	rmdir "$(DESTDIR)$(PREFIX)/share/licenses" 2>/dev/null || true
	rmdir "$(DESTDIR)$(PREFIX)/share/doc" 2>/dev/null || true
	rmdir "$(DESTDIR)$(PREFIX)/share" 2>/dev/null || true
	rmdir "$(BINDIR)" 2>/dev/null || true

dist: ## Source archive rvim-VERSION-source.tar.gz of HEAD (git archive; no tag, no upload)
	@v=$$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2); \
	git archive --format=tar.gz --prefix="rvim-$$v/" -o "rvim-$$v-source.tar.gz" HEAD && \
	echo "wrote rvim-$$v-source.tar.gz (the committed HEAD, not the working tree)"

cargo-install: ## Install into ~/.cargo/bin with cargo install --path . --locked (the old install)
	$(CARGO) install --path . --locked

clean: ## Remove build output (target/)
	$(CARGO) clean
