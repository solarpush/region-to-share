# TODO - Region to Share (Rust Implementation)

## ✅ Fait

### Interface utilisateur (egui)

- [x] Fenêtre principale avec contrôles
- [x] Mode sélection plein écran
- [x] Capture de screenshot pour sélection
- [x] Détection de la taille d'écran via backend
- [x] Interface de configuration
- [x] Sauvegarde/chargement des paramètres
- [x] Mode streaming pur (sans UI)

### Backend X11

- [x] Capture via XShm (shared memory)
- [x] Support multi-écrans
- [x] Capture de régions spécifiques
- [x] get_screen_size() pour détecter la taille de l'écran
- [x] capture_screenshot() pour capturer l'écran entier

### Configuration

- [x] Système de config JSON (~/.config/region-to-share/settings.json)
- [x] Frame rate, opacité, dernière région, etc.

## 🚧 En cours / Problèmes

### Fenêtre de streaming

- [ ] **BUG CRITIQUE**: Fenêtre ne se redimensionne pas correctement à la taille de la région
  - Problème: Le scaling entre screenshot affiché et coordonnées de sélection
  - Calcul du ratio scale_x/scale_y semble incorrect
  - La fenêtre doit faire EXACTEMENT la taille de la région sélectionnée (ex: 1000x1000 si sélection 1000x1000)

### Affichage du stream

- [ ] L'image streamée doit remplir toute la fenêtre (1:1, pas de scaling)
- [ ] Pas de padding, pas de marges
- [ ] Le contenu capturé doit correspondre pixel pour pixel à la région sélectionnée

## ❌ À faire

### Backend Portal/Wayland

- [ ] Implémenter réellement PortalCapture (actuellement c'est un stub)
  - [ ] Connexion DBus au portail XDG Desktop Portal
  - [ ] Création de session ScreenCast
  - [ ] Gestion des permissions utilisateur
  - [ ] Récupération du node_id PipeWire

- [ ] Implémenter réellement PipeWireStream
  - [ ] Connexion à PipeWire
  - [ ] Négociation du format vidéo
  - [ ] Réception des buffers DMA-BUF
  - [ ] Conversion BGRA/RGBA
  - [ ] get_stream_size() réel (obtenir la taille du stream PipeWire)

- [ ] capture_screenshot() pour Wayland/Portal
  - Actuellement retourne toujours 1920x1080
  - Doit capturer la vraie taille de l'écran Wayland

### Streaming et performance

- [ ] Vérifier que le frame rate est respecté
- [ ] Profiling des performances (FPS, latence)
- [ ] Optimisation de la capture X11
- [ ] Support DMA-BUF pour zero-copy (Wayland)

### UI et UX

- [ ] Hotkey global pour démarrer/arrêter (optionnel)
- [ ] Indicateur visuel du streaming en cours
- [ ] Gestion d'erreurs plus propre (messages à l'utilisateur)
- [ ] Mode "pause" du streaming
- [ ] Bouton pour revenir à l'interface de configuration depuis le mode streaming

### Tests et compatibilité

- [ ] Tester sur différents compositeurs Wayland (GNOME, KDE, Sway)
- [ ] Tester sur différentes résolutions (1080p, 1440p, 4K, ultra-wide)
- [ ] Tester multi-écrans
- [ ] Tests unitaires pour les conversions de coordonnées
- [ ] Tests d'intégration pour la capture

### Documentation

- [ ] Documenter l'architecture du code
- [ ] Guide d'utilisation complet
- [ ] Exemples d'utilisation avec OBS, Meet, Discord

## 🔧 Problèmes techniques à résoudre

### 1. **Conversion coordonnées sélection → région réelle**

**Problème actuel**: Les coordonnées de sélection sur le screenshot affiché en plein écran ne correspondent pas correctement aux pixels réels.

**Solution nécessaire**:

- Le screenshot capturé a une taille réelle (ex: 3440x1440 pour ultra-wide)
- La fenêtre plein écran peut avoir une taille différente
- Il faut calculer le ratio de scaling: `scale_x = screenshot_width / window_width`
- Appliquer ce ratio aux coordonnées de sélection
- **Actuellement implémenté mais ne fonctionne pas correctement**

**Code à vérifier**: `apply_selection()` dans `main.rs`

### 2. **Affichage du screenshot en mode sélection**

**Problème**: Le screenshot pourrait être scalé ou étiré dans la fenêtre plein écran.

**Solution**: Afficher le screenshot à sa taille native 1:1, avec scrolling si nécessaire, OU bien mémoriser le ratio de scaling exact appliqué par egui.

### 3. **Backend Portal manquant**

**Problème**: Le backend Wayland/Portal est un stub, pas d'implémentation réelle.

**Dépendances nécessaires**:

```toml
ashpd = "0.8"  # XDG Desktop Portal
pipewire = "0.8"  # PipeWire bindings
```

**Fichiers à implémenter**:

- `crates/region-portal/src/portal.rs` - Vraie implémentation DBus
- `crates/region-portal/src/pipewire.rs` - Vraie connexion PipeWire
- `crates/region-portal/src/stream.rs` - Gestion des buffers

### 4. **Redimensionnement de fenêtre**

**Problème**: `ViewportCommand::InnerSize()` ne semble pas redimensionner correctement.

**À vérifier**:

- Les commandes sont-elles exécutées dans le bon ordre?
- Y a-t-il un délai nécessaire entre Fullscreen(false) et InnerSize()?
- Le window manager respecte-t-il les demandes de resize?

## 📋 Priorités

### P0 - Critique (bloquant)

1. Fixer le redimensionnement de fenêtre après sélection
2. Affichage 1:1 du stream dans la fenêtre

### P1 - Important

3. Implémenter le backend Portal/Wayland fonctionnel
4. Tests sur différents environnements

### P2 - Nice to have

5. Optimisations de performance
6. UI/UX améliorée
7. Documentation complète

## 🎯 Objectif final

Une application qui:

1. Lance une capture plein écran du desktop
2. Affiche ce screenshot en plein écran
3. Permet de sélectionner une zone avec la souris
4. Redimensionne la fenêtre à la taille EXACTE de la zone sélectionnée
5. Stream cette zone en temps réel dans la fenêtre
6. La fenêtre peut être partagée dans Meet/Discord/OBS
7. Compatible X11 ET Wayland

**Comportement identique à la version Python existante.**
