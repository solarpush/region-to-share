# Guide de Build - Region to Share (Rust)

## Installation des dépendances

### Debian/Ubuntu

```bash
sudo apt install libx11-dev libxrandr-dev libxext-dev
```

### Fedora/RHEL

```bash
sudo dnf install libX11-devel libXrandr-devel libXext-devel
```

## Build

### Build en mode développement

```bash
cd rust-implementation
cargo build -p region-ui-egui
```

### Build en mode release (optimisé)

```bash
cd rust-implementation
cargo build -p region-ui-egui --release
```

### Exécution

```bash
# Mode développement
./target/debug/region-ui-egui

# Mode release
./target/release/region-ui-egui
```

## Configuration

Les paramètres sont sauvegardés dans :

```
~/.config/region-to-share/settings.json
```

### Paramètres disponibles

- **frame_rate** : Nombre de FPS (15-240, défaut: 60)
- **capture_mode** : Mode de capture (auto, x11, wayland)
- **show_performance** : Afficher les statistiques de performance
- **window_opacity** : Opacité de la fenêtre (0.1-1.0)
- **remember_last_region** : Se souvenir de la dernière région
- **auto_use_specific_region** : Utiliser automatiquement la dernière région
- **last_region** : Coordonnées de la dernière région (x, y, width, height)

### Exemple de configuration

```json
{
  "frame_rate": 60,
  "capture_mode": "auto",
  "show_performance": false,
  "window_opacity": 1.0,
  "auto_send_to_background": false,
  "remember_last_region": true,
  "auto_use_specific_region": false,
  "last_region": {
    "x": 100,
    "y": 100,
    "width": 800,
    "height": 600
  },
  "global_shortcut": ""
}
```

## Utilisation

1. **Lancer l'application**
2. **Cliquer sur "Selectionner region"**
   - La fenêtre passe en plein écran
   - Cliquez et glissez pour sélectionner une zone
   - Relâchez pour valider la sélection
   - Appuyez sur Échap pour annuler

3. **Streaming automatique**
   - La fenêtre se redimensionne à la taille de la zone sélectionnée
   - Le streaming démarre automatiquement
   - Partagez cette fenêtre dans Google Meet, Discord, OBS, etc.

4. **Paramètres**
   - Cliquez sur "⚙️ Parametres" pour accéder aux options
   - Ajustez le frame rate, l'opacité, etc.
   - Cliquez sur "💾 Sauvegarder" pour enregistrer les modifications

## Comparaison avec la version Python

### Avantages de la version Rust

- **Performance** : 2-3x plus rapide
- **Mémoire** : Utilisation mémoire réduite (~30-50% de moins)
- **Binaire unique** : Pas besoin de Python/PyQt5
- **Démarrage rapide** : Temps de démarrage instantané

### Flow identique

1. Capture de tout l'écran en plein écran
2. Sélection de la région avec la souris
3. Redimensionnement automatique de la fenêtre
4. Streaming de la zone sélectionnée

## Problèmes connus

- Support Wayland limité (nécessite xwayland)
- Nécessite X11 pour la capture d'écran

## Développement

### Structure du projet

```
rust-implementation/
├── crates/
│   ├── region-capture/    # Backend de capture (X11)
│   ├── region-config/     # Gestion de la configuration
│   ├── region-core/       # Types et structures de base
│   └── region-ui-egui/    # Interface utilisateur (egui)
└── target/               # Binaires compilés
```

### Ajouter des fonctionnalités

1. Modifier le code dans `crates/region-ui-egui/src/main.rs`
2. Recompiler : `cargo build -p region-ui-egui --release`
3. Tester : `./target/release/region-ui-egui`

### Tests

```bash
cargo test --workspace
```
