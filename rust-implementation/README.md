# Region to Share - Rust Implementation

**Version en développement** - Réécriture en Rust de l'application Region to Share pour de meilleures performances.

## État du projet

🚧 **En développement actif** - Voir [TODO.md](TODO.md) pour la liste complète des tâches.

### Fonctionnel
- ✅ Interface utilisateur de base (egui)
- ✅ Capture X11 (XShm)
- ✅ Sélection de région plein écran
- ✅ Configuration persistante
- ✅ Mode streaming sans UI

### En cours
- 🚧 Redimensionnement correct de fenêtre après sélection
- 🚧 Backend Wayland/Portal (stub seulement)

## Build rapide

```bash
# Dépendances (Debian/Ubuntu)
sudo apt install libx11-dev libxrandr-dev libxext-dev

# Compiler
cargo build -p region-ui-egui --release

# Exécuter
./target/release/region-ui-egui
```

## Utilisation

1. Cliquez sur "Selectionner region"
2. La fenêtre passe en plein écran avec un screenshot
3. Cliquez et glissez pour sélectionner une zone
4. La fenêtre se redimensionne et stream la zone
5. Partagez cette fenêtre dans Meet/Discord/OBS

## Architecture

```
crates/
├── region-core/      # Types de base (Rectangle, PixelFormat, etc.)
├── region-capture/   # Trait CaptureBackend + AutoBackend
│   └── x11/         # Implémentation X11/XShm
├── region-config/    # Configuration JSON
├── region-portal/    # Backend Wayland/Portal (WIP)
└── region-ui-egui/   # Interface utilisateur (egui)
```

## Développement

Voir [BUILD.md](BUILD.md) pour plus de détails sur le build.

Voir [TODO.md](TODO.md) pour les tâches en cours et à venir.

## Licence

Même licence que la version Python originale.
