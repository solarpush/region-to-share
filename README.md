# Region to Share

Une application Linux simple et efficace pour partager des zones spécifiques de votre écran dans les appels vidéo (Google Meet, Teams, Discord, etc.).

## 🚀 Fonctionnalités

- ✅ **Sélection interactive** de zone d'écran par cliquer-glisser
- ✅ **Fenêtre d'affichage en temps réel** (30 FPS) de la zone sélectionnée
- ✅ **Partage direct** dans les applications de visioconférence via "Partager fenêtre"
- ✅ **Contrôles intuitifs** : Pause/Reprise, Actualisation
- ✅ **Compatible** avec toutes les distributions Linux (X11/Wayland)
- ✅ **Léger et rapide** : Pas de dépendances complexes

## 🎯 Comment ça marche

1. **Lancez l'application** → Sélectionnez une zone d'écran
2. **Une fenêtre s'ouvre** → Affiche en temps réel le contenu de cette zone
3. **Dans votre app de visio** → "Partager l'écran" → "Fenêtre" → Sélectionnez "Region to Share"
4. **✅ Vous partagez uniquement cette zone !**

## 📋 Prérequis

- Linux (toute distribution moderne)
- Python 3.8+
- PyQt5 (installé automatiquement)

## 🔧 Installation

### Méthode 1: Installation simple

```bash
# Cloner le projet
git clone https://github.com/solarpush/region-to-share
cd region-to-share

# Installer les dépendances système
sudo apt update
sudo apt install python3-pyqt5 python3-pip python3-opencv python3-numpy python3-mss

# C'est tout ! Lancez l'application
./run.sh
```

### Méthode 2: Avec pip

````bash
# Cloner le projet
git clone https://github.com/solarpush/region-to-share
cd region-to-share

# Installer les dépendances Python
pip3 install -r requirements.txt

# Lancer l'application
## 🎯 Utilisation

### Lancement

```bash
./run.sh
````

### Étapes simples

1. **Lancer l'application** : `./run.sh`
2. **Sélectionner une zone** : Cliquez et glissez sur votre écran
3. **Fenêtre d'affichage** : Une fenêtre s'ouvre avec votre zone en temps réel
4. **Partager dans visioconférence** :
   - Google Meet/Teams/Discord : "Partager l'écran" → "Fenêtre"
   - Sélectionnez "Region to Share - Zone Sélectionnée"
   - ✅ Vous partagez uniquement cette zone !

### Contrôles

- **⏸️ Pause/▶️ Reprendre** : Arrêter/reprendre la capture
- **🔄 Actualiser** : Forcer une mise à jour
- **❌ Fermer** : Fermer l'application

## 🛠️ Architecture

```
region_to_share/
├── main.py              # Point d'entrée principal
├── screen_selector.py   # Sélection interactive de zone
├── display_window.py    # Fenêtre d'affichage temps réel
└── __init__.py          # Package Python
```

### Technologies

- **PyQt5** : Interface graphique moderne
- **mss** : Capture d'écran haute performance
- **OpenCV + NumPy** : Traitement d'image efficace
- **Snapcraft** : Empaquetage Linux universel

## 📦 Package Snap

### Construction du snap

```bash
# Installer snapcraft
sudo snap install snapcraft --classic

# Construire le snap
snapcraft

# Installer
sudo snap install --devmode *.snap
```

## 🤝 Contribution

1. Fork le projet
2. Créer une branche feature
3. Commit vos changements
4. Créer une Pull Request

## 📄 Licence

MIT License - voir le fichier [LICENSE](LICENSE) pour plus de détails.

---

**Region to Share** - Partage de zones d'écran simplifié pour Linux 🐧
