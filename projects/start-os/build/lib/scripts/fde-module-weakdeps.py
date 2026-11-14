#!/usr/bin/env python3

import json
import os
import runpy
import sys


if __name__ == "__main__":
    if len(sys.argv) < 5:
        sys.exit(
            "Usage: fde-module-weakdeps.py <admitted-library> "
            "<source-module-directory> <subjects-json> <config-path>..."
        )
    native = runpy.run_path(os.path.join(os.path.dirname(__file__), "fde-module-config.py"))
    with open(sys.argv[3], encoding="utf-8") as subjects_file:
        subjects = json.load(subjects_file)
    dependencies = native["module_weak_dependencies"](
        sys.argv[1], sys.argv[2], sys.argv[4:], subjects
    )
    print(json.dumps(dependencies))
