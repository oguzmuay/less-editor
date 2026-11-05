# LESS editor: build/run/install the Ghostty-based macOS app (app/ghostty is a git subtree).
GHOSTTY  := app/ghostty
ZIG_VER  := $(shell sed -n 's/.*minimum_zig_version = "\([0-9]*\.[0-9]*\)\..*/\1/p' $(GHOSTTY)/build.zig.zon)
BUILT    := $(GHOSTTY)/macos/build/ReleaseLocal/Ghostty.app
APP      := build/LESS.app
PREFIX   ?= /Applications
BINDIR   ?= $(HOME)/.local/bin
LESS_CFG := $(HOME)/.config/less
# native = this Mac's arch (fast); universal = arm64 + x86_64 (release builds).
XCFW     ?= native

.PHONY: app check run install uninstall clean dist

check:
	@command -v zig >/dev/null || { echo "Missing Zig $(ZIG_VER).x: brew install zig"; exit 1; }
	@zig version | grep -q '^$(subst .,\.,$(ZIG_VER))\.' || { echo "Zig $(ZIG_VER).x required, found $$(zig version)"; exit 1; }
	@xcodebuild -version >/dev/null 2>&1 || { echo "Full Xcode required (not just Command Line Tools): install Xcode, then sudo xcode-select -s /Applications/Xcode.app"; exit 1; }
	@xcrun metal --version >/dev/null 2>&1 || { echo "Metal Toolchain missing: xcodebuild -downloadComponent MetalToolchain"; exit 1; }
	@command -v nvim >/dev/null || echo "warning: Neovim not found (brew install neovim); LESS windows need it"

app: check
	cd $(GHOSTTY) && zig build -Doptimize=ReleaseFast -Dxcframework-target=$(XCFW) -Demit-macos-app=true
	app/scripts/brand.sh $(BUILT) $(APP)

run:
	@[ -d $(APP) ] || $(MAKE) app
	open -na $(APP)

install: app
	ditto $(APP) $(PREFIX)/LESS.app
	install -d $(BINDIR) && install -m 755 app/scripts/lessed $(BINDIR)/lessed
	@# Dev install: LESS uses this repo's nvim/ live (otherwise the first launch copies the bundled config).
	@[ -e $(LESS_CFG)/nvim ] || { mkdir -p $(LESS_CFG) && ln -s $(CURDIR)/nvim $(LESS_CFG)/nvim && echo "linked $(LESS_CFG)/nvim -> $(CURDIR)/nvim"; }
	@echo "Installed $(PREFIX)/LESS.app and $(BINDIR)/lessed"

# Release archives (zip + dmg) in build/dist; signs/notarizes only when LESS_SIGN_IDENTITY etc. are set.
dist:
	@[ -d $(APP) ] || $(MAKE) app
	app/scripts/package-macos.sh $(APP) build/dist

uninstall:
	rm -rf $(PREFIX)/LESS.app $(BINDIR)/lessed

clean:
	rm -rf build $(GHOSTTY)/macos/build $(GHOSTTY)/zig-out
