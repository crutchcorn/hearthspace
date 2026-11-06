"""Bounded ownership of a fixture's inherited process group, not daemon escape."""

import contextlib

from portal_checklib import OwnedProcess


@contextlib.contextmanager
def owned_process(command, *, term_timeout=3, kill_timeout=3, **kwargs):
    process = OwnedProcess(command, **kwargs)
    try:
        yield process
    finally:
        process.stop(term_timeout=term_timeout, kill_timeout=kill_timeout)
