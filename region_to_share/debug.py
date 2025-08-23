import os


def debug_print(message):
    """Print debug message if debug mode is enabled"""
    if os.environ.get("REGION_TO_SHARE_DEBUG"):
        print(f"🐛 DEBUG: {message}")
