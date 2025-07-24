#!/bin/bash

# Script de lancement pour Region to Share
# Utilise l'environnement virtuel embarqué

# Répertoire du script
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Vérifier que le venv existe
if [ ! -d "$SCRIPT_DIR/venv_region" ]; then
    echo "❌ Environnement virtuel non trouvé !"
    echo "📋 Veuillez d'abord installer les dépendances :"
    echo "   python3 -m venv venv_region"
    echo "   source venv_region/bin/activate"
    echo "   pip install -r requirements.txt"
    exit 1
fi

# Activer l'environnement virtuel et lancer l'application
echo "🚀 Lancement de Region to Share..."
source "$SCRIPT_DIR/venv_region/bin/activate"
exec python "$SCRIPT_DIR/region_to_share/main.py" "$@"
