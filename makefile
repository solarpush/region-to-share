NAME = region-to-share
VERSION = $(shell node -p "require('./package.json').version")
SNAP_PATH = dist/region-to-share_$(VERSION)_amd64.snap

.PHONY: all clean build upload release patch dev-install

# 🔄 Clean build
clean:
	rm -rf dist

# ⚙️ Build snap avec electron-builder
build:
	npm run dist

# 🚀 Upload vers Snapcraft
upload: build
	snapcraft upload $(SNAP_PATH)

# ✅ Release dans le canal edge (modifiable)
release:
	@snapcraft status $(NAME)
	@echo "🔍 Choisis une révision dans la liste ci-dessus (ex: 5), puis relance avec:"
	@echo "    make release-do REVISION=5"

release-do:
ifndef REVISION
	$(error 🔴 Tu dois spécifier une révision: make release-do REVISION=<id>)
endif
	snapcraft release $(NAME) $(REVISION) edge

# 🧪 Incrémentation automatique de patch version
patch:
	npm version patch --no-git-tag-version
dev-install:
	snap install --dangerous dist/region-to-share_$(VERSION)*.snap  