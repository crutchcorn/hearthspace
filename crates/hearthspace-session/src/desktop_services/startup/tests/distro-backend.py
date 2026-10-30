#!/usr/bin/python3
"""Record a distro-name activation attempt, then exit without a lasting child."""
from pathlib import Path

Path(__file__).with_name("distro-activated").write_text("activated\n")
raise SystemExit(1)
