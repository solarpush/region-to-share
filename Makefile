.PHONY: help clean build upload release install test init_venv install_gi setup test_deps

# Variables
SNAP_NAME = region-to-share
VERSION = 1.0.7
SNAP_FILE = $(SNAP_NAME)_$(VERSION)_amd64.snap

help:
	@echo "Available commands:"
	@echo "  make setup     - Complete setup (install_gi + init_venv)"
	@echo "  make build     - Build the snap package"
	@echo "  make upload    - Upload snap to store (requires build first)"
	@echo "  make release   - Build and upload snap to store in one command"
	@echo "  make clean     - Clean build artifacts"
	@echo "  make install   - Install the snap locally"
	@echo "  make test      - Test the local installation"
	@echo "  make test_deps - Test Python dependencies"

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
	rm -rf venv_region
	python3.12 -m venv --system-site-packages venv_region
	. ./venv_region/bin/activate; \
	pip install --upgrade pip; \
	pip install -r requirements.txt

install_gi:
	@echo "Installing GObject Introspection..."
	sudo apt update
	sudo apt-get install -y python3-gi python3-gi-cairo gir1.2-gstreamer-1.0 gir1.2-gst-plugins-base-1.0

setup: install_gi init_venv
	@echo "✅ Complete setup finished"
	@echo "You can now run: ./run_venv.sh"

test_deps:
	@echo "Testing Python dependencies..."
	. ./venv_region/bin/activate; \
	python -c "import gi; print('✅ gi available')"; \
	python -c "gi.require_version('Gst', '1.0'); from gi.repository import Gst; print('✅ GStreamer available')"; \
	python -c "import PyQt5; print('✅ PyQt5 available')"; \
	python -c "import mss; print('✅ MSS available')"; \
	python -c "import dbus; print('✅ dbus-python available')"; \
	python -c "import psutil; print('✅ psutil available')"; \
	echo "✅ All dependencies OK!"