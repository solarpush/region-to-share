APP_ID          := io.github.solarpush.RegionToShare
MANIFEST_LOCAL  := $(APP_ID).local.yml
MANIFEST_DEPLOY := $(APP_ID).yml
BUILDDIR        := build-flatpak
VERSION         := 3.0.0

.PHONY: help deps cargo-sources build install run clean uninstall validate lint \
        build-native test package-kde package-gnome install-kde install-gnome doctor

help: ## Affiche cette aide
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) \
		| awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-18s\033[0m %s\n", $$1, $$2}'

# ── Build Natif & Tests ────────────────────────────────────────────────────────

build-native: ## Compile le binaire natif `region-to-share` (release)
	cargo build --release -p region-cli

test: ## Exécute tous les tests du workspace
	cargo test --workspace

doctor: ## Exécute le diagnostic système (Wayland Portal v6, KDE, PipeWire, FFmpeg)
	cargo run -p region-cli -- doctor

# ── Composants KDE Plasma 6 & GNOME Shell ─────────────────────────────────────

package-kde: ## Construit l'archive .plasmoid distribuable pour KDE Store / Discover
	mkdir -p dist
	cd integrations/kde-plasmoid && zip -r ../../dist/org.kde.plasma.regiontoshare-$(VERSION).plasmoid .
	@echo "✓ Created dist/org.kde.plasma.regiontoshare-$(VERSION).plasmoid"

package-gnome: ## Construit le zip distribuable pour extensions.gnome.org (EGO)
	mkdir -p dist
	glib-compile-schemas integrations/gnome-extension/schemas || true
	cd integrations/gnome-extension && zip -r ../../dist/region-to-share@solarpush.github.io-$(VERSION).shell-extension.zip .
	@echo "✓ Created dist/region-to-share@solarpush.github.io-$(VERSION).shell-extension.zip"

install-kde: ## Installe le Plasmoid KDE Plasma 6 + service D-Bus localement
	cargo run -p region-cli -- integrate install-kde

install-gnome: ## Installe l'extension GNOME Shell + service D-Bus localement
	cargo run -p region-cli -- integrate install-gnome

# ── Flatpak ────────────────────────────────────────────────────────────────────

deps: ## Installe le SDK Flatpak + extensions
	flatpak remote-add --user --if-not-exists flathub https://flathub.org/repo/flathub.flatpakrepo || true
	flatpak install --user -y flathub \
		org.freedesktop.Platform//25.08 \
		org.freedesktop.Sdk//25.08 \
		org.freedesktop.Sdk.Extension.rust-stable//25.08 \
		org.freedesktop.Sdk.Extension.llvm20//25.08 \
		org.flatpak.Builder

cargo-sources: ## Régénère cargo-sources.json depuis Cargo.lock
	flatpak-cargo-generator Cargo.lock -o cargo-sources.json

build: ## Build Flatpak local sans installer
	flatpak-builder --force-clean $(BUILDDIR) $(MANIFEST_LOCAL)

install: ## Build + installe Flatpak en mode user
	flatpak-builder --force-clean --user --install $(BUILDDIR) $(MANIFEST_LOCAL)

build-deploy: ## Build avec le manifest de déploiement Flathub
	flatpak-builder --force-clean $(BUILDDIR) $(MANIFEST_DEPLOY)

install-deploy: ## Build + install avec le manifest de déploiement Flathub
	flatpak-builder --force-clean --user --install $(BUILDDIR) $(MANIFEST_DEPLOY)

run: ## Lance l'app Flatpak installée
	flatpak run $(APP_ID)

run-build: build ## Lance directement depuis le builddir local
	flatpak-builder --run $(BUILDDIR) $(MANIFEST_LOCAL) region-to-share

validate: ## Valide le fichier metainfo avec appstreamcli
	appstreamcli validate --pedantic $(APP_ID).metainfo.xml

lint: ## Vérifie le manifest de déploiement avec flatpak-builder-lint
	flatpak run --command=flatpak-builder-lint org.flatpak.Builder manifest $(MANIFEST_DEPLOY)

clean: ## Supprime builddir, cache Flatpak et dist/
	rm -rf $(BUILDDIR) .flatpak-builder dist

uninstall: ## Désinstalle l'app Flatpak
	flatpak uninstall --user -y $(APP_ID)
