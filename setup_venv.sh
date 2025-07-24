#!/bin/bash

# Script d'installation automatique pour Region to Share
# Configure un environnement virtuel complet et autonome

set -e  # Arrêter en cas d'erreur

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
VENV_DIR="$SCRIPT_DIR/venv_region"

echo "🔧 Installation de Region to Share..."

# Vérifier Python 3
if ! command -v python3 &> /dev/null; then
    echo "❌ Python 3 n'est pas installé"
    echo "📋 Installez Python 3 avec : sudo apt install python3 python3-venv python3-pip"
    exit 1
fi

# Créer l'environnement virtuel s'il n'existe pas
if [ ! -d "$VENV_DIR" ]; then
    echo "📦 Création de l'environnement virtuel..."
    python3 -m venv "$VENV_DIR"
fi

# Activer l'environnement virtuel
echo "🔄 Activation de l'environnement virtuel..."
source "$VENV_DIR/bin/activate"

# Mettre à jour pip
echo "⬆️  Mise à jour de pip..."
pip install --upgrade pip

# Installer les dépendances
echo "📚 Installation des dépendances..."
pip install -r "$SCRIPT_DIR/requirements.txt"

echo ""
echo "✅ Installation terminée !"
echo ""
echo "🚀 Pour lancer l'application :"
echo "   ./run_venv.sh"
echo ""
echo "📂 L'environnement virtuel est dans : venv_region/"
echo "💾 Taille totale : $(du -sh "$VENV_DIR" | cut -f1)"
