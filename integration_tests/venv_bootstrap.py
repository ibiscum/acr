#!/usr/bin/env python3
"""Ensure integration tests run with integration_tests/.venv when available."""

from __future__ import annotations

import os
import sys
from pathlib import Path


def _venv_python_path() -> Path:
    test_dir = Path(__file__).resolve().parent
    venv_dir = test_dir / ".venv"
    if os.name == "nt":
        return venv_dir / "Scripts" / "python.exe"
    return venv_dir / "bin" / "python"


def _venv_dir() -> Path:
    return _venv_python_path().parent.parent


def ensure_venv_python() -> None:
    """Re-exec with integration_tests/.venv Python unless already using it."""
    if os.environ.get("ACR_INTEGRATION_VENV_ACTIVE") == "1":
        return

    # When running via pytest, do not re-exec during test module imports.
    # Users should invoke pytest from the desired interpreter/activated venv.
    original_argv = getattr(sys, "orig_argv", None) or []
    joined_argv = " ".join(original_argv + sys.argv)
    if "pytest" in joined_argv:
        return

    venv_python = _venv_python_path()
    if not venv_python.exists():
        return

    # Do not use Path.resolve() for interpreter comparison because venv Python
    # may be a symlink to system Python on Debian, which would hide venv usage.
    if sys.prefix != sys.base_prefix and Path(sys.prefix).resolve() == _venv_dir().resolve():
        return

    env = os.environ.copy()
    env["ACR_INTEGRATION_VENV_ACTIVE"] = "1"
    original_argv = getattr(sys, "orig_argv", None)
    if original_argv and len(original_argv) > 1:
        argv = [str(venv_python), *original_argv[1:]]
    else:
        argv = [str(venv_python), *sys.argv]
    os.execve(str(venv_python), argv, env)


ensure_venv_python()
