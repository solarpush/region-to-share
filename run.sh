#!/bin/bash

# Script de lancement pour Region to Share
# Utilise le Python système pour avoir accès à PyQt5

# Répertoire du script
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Variables d'environnement pour éviter les conflits Qt
export QT_QPA_PLATFORM_PLUGIN_PATH=""
export QT_PLUGIN_PATH="/usr/lib/x86_64-linux-gnu/qt5/plugins"

# Lancer avec le Python système
echo "🚀 Lancement de Region to Share..."
exec /usr/bin/python3 "$SCRIPT_DIR/region_to_share/main.py" "$@"
