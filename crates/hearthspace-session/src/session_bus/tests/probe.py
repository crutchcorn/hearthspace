"""D-Bus activation probe: only fixture-requested keys are written, never a dump."""
import json
import os
import sys

output, *keys = sys.argv[1:]
with open(output + ".tmp", "x", encoding="utf-8") as report:
    json.dump({key: os.environ.get(key) for key in keys}, report)
os.replace(output + ".tmp", output)
# No service name is acquired: StartServiceByName must report this child's exit.
