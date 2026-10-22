"""
Loads a Python project config and writes it as JSON for the C++ engine.

    python3 -m uvmcheck.config_runner --config uvmcheck_config.py --out cfg.json

The config file may define ``config = SomeProject()`` explicitly. Otherwise the
runner picks the single most-derived ProjectConfig subclass defined in the
file (base classes imported from shared files are not candidates).
"""

import argparse
import importlib.util
import inspect
import json
import os
import sys
import traceback

from .config import ConfigError, ProjectConfig


def load_config(path: str) -> ProjectConfig:
    path = os.path.abspath(path)
    config_dir = os.path.dirname(path)
    # Let configs import shared team bases sitting next to them.
    if config_dir not in sys.path:
        sys.path.insert(0, config_dir)

    module_name = "_uvmcheck_project_config"
    spec = importlib.util.spec_from_file_location(module_name, path)
    if not spec or not spec.loader:
        raise ConfigError(f"cannot import {path}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[module_name] = module
    spec.loader.exec_module(module)

    explicit = getattr(module, "config", None)
    if explicit is not None:
        if inspect.isclass(explicit) and issubclass(explicit, ProjectConfig):
            return explicit()
        if isinstance(explicit, ProjectConfig):
            return explicit
        raise ConfigError("'config' must be a ProjectConfig subclass or instance")

    local = [obj for _, obj in inspect.getmembers(module, inspect.isclass)
             if issubclass(obj, ProjectConfig) and obj is not ProjectConfig
             and obj.__module__ == module_name]
    leaves = [c for c in local if not any(o is not c and issubclass(o, c) for o in local)]
    if not leaves:
        raise ConfigError("no ProjectConfig subclass found; define one or set 'config = MyProject'")
    if len(leaves) > 1:
        names = ", ".join(sorted(c.__name__ for c in leaves))
        raise ConfigError(f"several ProjectConfig classes found ({names}); set 'config = <one of them>'")
    return leaves[0]()


def main() -> int:
    parser = argparse.ArgumentParser(description="UVMCheck Python config loader")
    parser.add_argument("--config", required=True, help="Path to the Python config file")
    parser.add_argument("--out", required=True, help="Where to write the JSON form")
    args = parser.parse_args()

    try:
        cfg = load_config(args.config)
        data = cfg.to_dict()
        data["source"] = type(cfg).__qualname__
    except ConfigError as e:
        sys.stderr.write(f"{args.config}: {e}\n")
        return 1
    except Exception:
        sys.stderr.write(traceback.format_exc())
        return 1

    with open(args.out, "w") as f:
        json.dump(data, f, indent=2)
    return 0


if __name__ == "__main__":
    sys.exit(main())
