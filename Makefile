APP_ID          := io.github.solarpush.RegionToShare
MANIFEST_LOCAL  := $(APP_ID).local.yml   # type: dir  – build local
MANIFEST_DEPLOY := $(APP_ID).yml         # type: git  – soumission Flathub
BUILDDIR        := build-flatpak

.PHONY: help deps cargo-sources build install run clean uninstall validate lint

help: ## Affiche cette aide
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) \
		| awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-18s\033[0m %s\n", $$1, $$2}'

# ── Dépendances ────────────────────────────────────────────────────────────────

deps: ## Installe le SDK Flatpak + extensions (à faire une seule fois)
	@# Ajoute le remote flathub user si absent
	flatpak remote-add --user --if-not-exists flathub https://flathub.org/repo/flathub.flatpakrepo || true
	flatpak install --user -y flathub \
		org.freedesktop.Platform//25.08 \
		org.freedesktop.Sdk//25.08 \
		org.freedesktop.Sdk.Extension.rust-stable//25.08 \
		org.freedesktop.Sdk.Extension.llvm20//25.08 \
		org.flatpak.Builder

cargo-sources: ## Régénère cargo-sources.json depuis Cargo.lock
	flatpak-cargo-generator Cargo.lock -o cargo-sources.json

# ── Build ──────────────────────────────────────────────────────────────────────

build: ## Build local sans installer (rapide, sources locales)
	flatpak-builder --force-clean $(BUILDDIR) $(MANIFEST_LOCAL)

install: ## Build + installe l'app en mode user (sources locales)
	flatpak-builder --force-clean --user --install $(BUILDDIR) $(MANIFEST_LOCAL)

build-deploy: ## Build avec le manifest de déploiement (type: git – test avant Flathub)
	flatpak-builder --force-clean $(BUILDDIR) $(MANIFEST_DEPLOY)

install-deploy: ## Build + install avec le manifest de déploiement
	flatpak-builder --force-clean --user --install $(BUILDDIR) $(MANIFEST_DEPLOY)

# ── Run ────────────────────────────────────────────────────────────────────────

run: ## Lance l'app installée
	flatpak run $(APP_ID)

run-build: build ## Lance directement depuis le builddir local (sans installer)
	flatpak-builder --run $(BUILDDIR) $(MANIFEST_LOCAL) region-to-share

# ── Qualité ────────────────────────────────────────────────────────────────────

validate: ## Valide le fichier metainfo avec appstreamcli
	appstreamcli validate --pedantic $(APP_ID).metainfo.xml

lint: ## Vérifie le manifest de déploiement avec flatpak-builder-lint (Flathub)
	flatpak run --command=flatpak-builder-lint org.flatpak.Builder manifest $(MANIFEST_DEPLOY)

lint-local: ## Vérifie le manifest local (filename-mismatch attendu)
	flatpak run --command=flatpak-builder-lint org.flatpak.Builder manifest $(MANIFEST_LOCAL)

# ── Nettoyage ──────────────────────────────────────────────────────────────────

clean: ## Supprime le builddir et le cache .flatpak-builder
	rm -rf $(BUILDDIR) .flatpak-builder

uninstall: ## Désinstalle l'app
	flatpak uninstall --user -y $(APP_ID)
