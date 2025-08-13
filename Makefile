.PHONY: help clean build upload release install test init_venv

# Variables
SNAP_NAME = region-to-share
VERSION = 1.0.5
SNAP_FILE = $(SNAP_NAME)_$(VERSION)_amd64.snap

help:
	@echo "Available commands:"
	@echo "  make build     - Build the snap package"
	@echo "  make upload    - Upload snap to store (requires build first)"
	@echo "  make release   - Build and upload snap to store in one command"
	@echo "  make clean     - Clean build artifacts"
	@echo "  make install   - Install the snap locally"
	@echo "  make test      - Test the local installation"

clean:
	@echo "Cleaning build artifacts..."
	snapcraft clean
	rm -f *.snap

build:
	@echo "Building snap package..."
	snapcraft --use-lxd

upload:
	@echo "Uploading $(SNAP_FILE) to Snap Store..."
	snapcraft upload $(SNAP_FILE) --release=stable

release: clean build upload
	@echo "✅ Build and upload completed successfully"

install:
	@echo "Installing $(SNAP_FILE) locally..."
	sudo snap install $(SNAP_FILE) --dangerous

test:
	@echo "Testing region-to-share installation..."
	snap list | grep region-to-share
	@echo "Try running: region-to-share"

init_venv:
	@echo "Initializing virtual environment..."
	python3 -m venv venv_region
	. ./venv_region/bin/activate; \
    pip install --upgrade pip; \
    pip install -r requirements.txt