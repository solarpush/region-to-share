# Region-to-Share Linux Snap 📺

**Region-to-Share** est une application Linux qui permet de sélectionner une zone spécifique de votre écran et de l'afficher dans une fenêtre dédiée, parfaite pour le partage dans les applications de visioconférence.

## 🚀 Installation

### Via Snap (recommandé)

```bash
# Installation depuis le fichier local
sudo snap install --dangerous region-to-share_1.0.0_amd64.snap

# Ou depuis le Snap Store (quand publié)
sudo snap install region-to-share
```

### Exécution manuelle

```bash
# Cloner le projet
git clone https://github.com/solarpush/region-to-share.git
cd region-to-share

# Activer l'environnement virtuel
source venv_region/bin/activate

# Lancer l'application
python -m region_to_share.main
```

## ✨ Fonctionnalités

- **Sélection intuitive** : Cliquez et glissez pour sélectionner une zone d'écran
- **Fenêtre d'affichage** : Taille exacte de la région sélectionnée (sans bordures)
- **Capture temps réel** : 30 FPS avec curseur visible dans la zone
- **Contrôles overlay** : Pause/lecture, actualisation, fermeture (au survol)
- **Compatible** : Fonctionne avec Google Meet, Teams, Discord, Zoom, etc.

## 🎯 Utilisation

1. **Lancer l'application**

   ```bash
   region-to-share
   ```

2. **Sélectionner une zone**

   - L'écran devient sombre avec une interface de sélection
   - Cliquez et glissez pour définir la zone à partager
   - Relâchez pour confirmer la sélection

3. **Partager dans votre visioconférence**

   - Une fenêtre s'ouvre avec la zone sélectionnée
   - Dans Google Meet/Teams, cliquez sur "Partager l'écran"
   - Sélectionnez la fenêtre "Region to Share - Zone Sélectionnée"

4. **Contrôles disponibles**
   - **⏸️ Pause** : Mettre en pause la capture
   - **🔄 Actualiser** : Forcer une mise à jour
   - **❌ Fermer** : Arrêter le partage

## 🔧 Permissions requises

Le snap demande les permissions suivantes :

- `desktop` : Interface graphique
- `x11` / `wayland` : Capture d'écran
- `home` : Accès aux fichiers utilisateur (optionnel)

## 🐛 Résolution de problèmes

### L'application ne démarre pas

```bash
# Vérifier les logs
journalctl --user -f | grep region-to-share
```

### Pas de capture d'écran

- Vérifiez que les permissions sont accordées
- Sur Wayland, certaines restrictions peuvent s'appliquer

### Performance

- Réduisez la taille de la zone si l'ordinateur rame
- Fermez les autres applications gourmandes

## 🏗️ Développement

### Construire le snap

```bash
# Installation des dépendances
sudo snap install snapcraft --classic

# Construction
snapcraft --destructive-mode

# Installation locale
sudo snap install --dangerous region-to-share_1.0.0_amd64.snap
```

### Structure du projet

```
region-to-share/
├── region_to_share/
│   ├── main.py              # Point d'entrée
│   ├── screen_selector.py   # Sélection de zone
│   └── display_window.py    # Fenêtre d'affichage
├── venv_region/             # Environnement virtuel
├── requirements.txt         # Dépendances Python
├── snapcraft.yaml          # Configuration snap
└── region-to-share.desktop  # Fichier desktop
```

## 📝 Licence

Ce projet est sous licence MIT - voir le fichier [LICENSE](LICENSE) pour plus de détails.

## 🤝 Contribution

Les contributions sont les bienvenues ! N'hésitez pas à :

- Signaler des bugs
- Proposer des améliorations
- Soumettre des pull requests

## 📧 Support

Pour toute question ou problème :

- Ouvrir une issue sur GitHub
- Consulter la documentation
- Vérifier les permissions snap

---

**Bon partage d'écran ! 🎉**
