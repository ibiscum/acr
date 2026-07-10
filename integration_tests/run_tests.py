#!/usr/bin/env python3
"""
Test runner for AudioControl integration tests
"""

import os
import subprocess
import sys
from pathlib import Path

TEST_DIR = Path(__file__).parent
VENV_DIR = TEST_DIR / ".venv"


def venv_python_path() -> Path:
    """Return platform-specific path to the venv Python executable."""
    if os.name == "nt":
        return VENV_DIR / "Scripts" / "python.exe"
    return VENV_DIR / "bin" / "python"


def active_python() -> str:
    """Return the Python executable that should run installs/tests."""
    venv_python = venv_python_path()
    if venv_python.exists():
        return str(venv_python)
    return sys.executable


def activate_venv_env_vars() -> None:
    """Expose environment variables equivalent to activating the venv."""
    venv_bin = str(venv_python_path().parent)
    os.environ["VIRTUAL_ENV"] = str(VENV_DIR)
    os.environ["PATH"] = f"{venv_bin}{os.pathsep}{os.environ.get('PATH', '')}"


def ensure_venv() -> bool:
    """Create a local virtual environment if it does not exist."""
    if venv_python_path().exists():
        return True

    print(f"Creating virtual environment at {VENV_DIR}...")
    result = subprocess.run([
        sys.executable,
        "-m",
        "venv",
        str(VENV_DIR),
    ], capture_output=True, text=True)

    if result.returncode != 0:
        print(f"Failed to create virtual environment: {result.stderr}")
        return False

    print("Virtual environment created successfully")
    return True


def relaunch_in_venv() -> None:
    """Relaunch this script with the .venv interpreter once."""
    venv_python = venv_python_path()
    current_python = Path(sys.executable).resolve()

    if current_python == venv_python.resolve():
        return

    if os.environ.get("ACR_INTEGRATION_VENV_ACTIVE") == "1":
        return

    env = os.environ.copy()
    env["ACR_INTEGRATION_VENV_ACTIVE"] = "1"

    print(f"Re-launching with virtual environment interpreter: {venv_python}")
    result = subprocess.run([str(venv_python), __file__, *sys.argv[1:]], env=env)
    sys.exit(result.returncode)

def ensure_dependencies():
    """Ensure Python dependencies are installed"""
    requirements_file = TEST_DIR / "requirements.txt"

    print("Installing Python dependencies...")
    result = subprocess.run([
        active_python(), "-m", "pip", "install", "-r", str(requirements_file)
    ], capture_output=True, text=True)

    if result.returncode != 0:
        print(f"Failed to install dependencies: {result.stderr}")
        return False

    print("Dependencies installed successfully")
    return True

def build_audiocontrol():
    """Build the AudioControl binary"""
    print("Building AudioControl binary...")

    # Change to project root directory (one level up from tests)
    project_root = Path(__file__).parent.parent

    result = subprocess.run([
        "cargo", "build"
    ], cwd=str(project_root), capture_output=True, text=True)

    if result.returncode != 0:
        print(f"Failed to build AudioControl: {result.stderr}")
        return False

    print("AudioControl built successfully")
    return True

def run_tests():
    """Run the integration tests"""
    test_dir = TEST_DIR

    print("Running integration tests...")

    # Run all discovered test files to avoid stale hardcoded names.
    test_files = sorted(path.name for path in test_dir.glob("test_*.py"))
    if not test_files:
        print("No integration test files found")
        return False

    all_passed = True

    for test_file in test_files:
        test_path = test_dir / test_file

        print(f"\n{'='*50}")
        print(f"Running {test_file}")
        print(f"{'='*50}")

        result = subprocess.run([
            active_python(), "-m", "pytest", str(test_path), "-v", "--tb=short"
        ])

        if result.returncode != 0:
            all_passed = False
            print(f"FAILED: {test_file}")
        else:
            print(f"PASSED: {test_file}")

    return all_passed

def main():
    """Main function"""
    print("AudioControl Integration Test Runner")
    print("=" * 40)

    # Bootstrap and switch into the local virtual environment.
    if not ensure_venv():
        return 1
    activate_venv_env_vars()
    relaunch_in_venv()

    # Change to the tests directory
    os.chdir(TEST_DIR)

    # Step 1: Install dependencies
    if not ensure_dependencies():
        return 1

    # Step 2: Build AudioControl
    if not build_audiocontrol():
        return 1

    # Step 3: Run tests
    if run_tests():
        print("\n" + "=" * 40)
        print("All tests passed!")
        return 0
    else:
        print("\n" + "=" * 40)
        print("Some tests failed!")
        return 1

if __name__ == "__main__":
    sys.exit(main())
