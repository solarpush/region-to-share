#!/usr/bin/env python3
"""Setup script for region-to-share."""

from setuptools import setup, find_packages
import pathlib

here = pathlib.Path(__file__).parent.resolve()

# Get the long description from the README file
long_description = (here / "README.md").read_text(encoding="utf-8")

setup(
    name="region-to-share",
    version="1.0.5",
    description="Real-time screen region capture and sharing for GNOME Wayland",
    long_description=long_description,
    long_description_content_type="text/markdown",
    url="https://github.com/solarpush/region-to-share",
    author="Pierre Solar",
    author_email="pierre@solarpush.dev",
    classifiers=[
        "Development Status :: 4 - Beta",
        "Intended Audience :: End Users/Desktop",
        "Topic :: Multimedia :: Graphics :: Capture :: Screen Capture",
        "Topic :: Communications :: Video Conferencing",
        "License :: OSI Approved :: MIT License",
        "Programming Language :: Python :: 3",
        "Programming Language :: Python :: 3.10",
        "Programming Language :: Python :: 3.11",
        "Programming Language :: Python :: 3.12",
        "Programming Language :: Python :: 3.13",
        "Operating System :: POSIX :: Linux",
        "Environment :: X11 Applications :: Qt",
        "Environment :: Wayland",
    ],
    keywords="screen-capture, wayland, gnome, pipewire, video-conferencing, real-time",
    packages=find_packages(),
    python_requires=">=3.10",
    install_requires=[
        "PyQt5>=5.15.0",
        "PyQt5-stubs>=5.15.6.0",
        "mss>=10.0.0",
        "opencv-python-headless>=4.12.0",
        "numpy>=2.0.0",
        "pyscreenshot>=3.1",
        "Pillow>=11.0.0",
        "dbus-python>=1.3.0",
        "python-xlib>=0.33",
        "jeepney>=0.8.0",
        "six>=1.17.0",
        "EasyProcess>=1.1",
        "entrypoint2>=1.1",
        "pycairo>=1.28.0",
    ],
    extras_require={
        "dev": [
            "pytest>=7.0.0",
            "pytest-qt>=4.2.0",
            "pytest-mock>=3.10.0",
            "black>=23.0.0",
            "flake8>=6.0.0",
            "mypy>=1.0.0",
            "build>=0.10.0",
            "wheel>=0.40.0",
            "twine>=4.0.0",
            "sphinx>=6.0.0",
            "sphinx-rtd-theme>=1.2.0",
        ]
    },
    entry_points={
        "console_scripts": [
            "py-region-to-share=region_to_share.main:main",
        ],
    },
    project_urls={
        "Bug Reports": "https://github.com/solarpush/region-to-share/issues",
        "Source": "https://github.com/solarpush/region-to-share",
        "Documentation": "https://github.com/solarpush/region-to-share/blob/main/README.md",
    },
    include_package_data=True,
    package_data={
        "region_to_share": ["*.png", "*.svg", "*.desktop"],
    },
)
