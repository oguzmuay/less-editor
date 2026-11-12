CARGO ?= cargo
PREFIX ?= /Applications

.PHONY: check test clippy fmt app dist run install clean

check:
	$(CARGO) check --workspace

test:
	$(CARGO) test --workspace

clippy:
	$(CARGO) clippy --workspace --all-targets -- -D warnings

fmt:
	$(CARGO) fmt --all

app:
	scripts/package-macos.sh --app-only

dist:
	scripts/package-macos.sh

run:
	$(CARGO) run -p less-app

install: app
	ditto build/LESS.app $(PREFIX)/LESS.app
	@echo "Installed $(PREFIX)/LESS.app (ad-hoc signed; not notarized)"

clean:
	rm -rf build
	$(CARGO) clean
