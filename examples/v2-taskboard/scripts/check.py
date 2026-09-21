"""Run a disposable Compose baseline; never prune shared Docker resources."""
import pathlib
import subprocess
import sys
import uuid


def main():
    root = pathlib.Path(__file__).resolve().parents[1]
    project = "zforge-tb-" + uuid.uuid4().hex[:12]
    command = ["docker", "compose", "--project-name", project, "--file", str(root / "compose.yaml")]
    print("Fixture environment: " + project, flush=True)
    result = 1
    try:
        result = subprocess.run(command + ["up", "--build", "--abort-on-container-exit",
                                          "--exit-code-from", "checks"], cwd=root, timeout=300).returncode
    except (OSError, subprocess.TimeoutExpired, KeyboardInterrupt) as error:
        print("Fixture interrupted: {}".format(type(error).__name__), file=sys.stderr)
    finally:
        try:
            cleanup = subprocess.run(command + ["down", "--volumes", "--remove-orphans"], cwd=root, timeout=60)
            cleaned = cleanup.returncode == 0
        except (OSError, subprocess.TimeoutExpired, KeyboardInterrupt):
            cleaned = False
        if not cleaned:
            print("Cleanup failed for " + project + "; reconcile this exact project manually.", file=sys.stderr)
            result = 1
    return result


if __name__ == "__main__":
    sys.exit(main())
