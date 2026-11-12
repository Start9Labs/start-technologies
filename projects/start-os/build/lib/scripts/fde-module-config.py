#!/usr/bin/env python3

import ctypes
import os
import sys


def module_commands(library_path, module_directory, configuration_paths):
    # The caller admits the stable source library before loading it.
    library = ctypes.CDLL(library_path)
    pointer = ctypes.c_void_p
    text = ctypes.c_char_p
    signatures = {
        "kmod_new": (pointer, [text, ctypes.POINTER(text)]),
        "kmod_unref": (pointer, [pointer]),
        "kmod_config_iter_next": (ctypes.c_bool, [pointer]),
        "kmod_config_iter_get_key": (text, [pointer]),
        "kmod_config_iter_free_iter": (None, [pointer]),
        "kmod_config_get_install_commands": (pointer, [pointer]),
        "kmod_config_get_remove_commands": (pointer, [pointer]),
    }
    for name, (result, arguments) in signatures.items():
        function = getattr(library, name)
        function.restype = result
        function.argtypes = arguments

    paths = (text * (len(configuration_paths) + 1))(
        *(os.fsencode(path) for path in configuration_paths), None
    )
    context = library.kmod_new(os.fsencode(module_directory), paths)
    if not context:
        raise RuntimeError("Cannot create libkmod configuration context")
    try:
        for kind in ("install", "remove"):
            iterator = getattr(library, f"kmod_config_get_{kind}_commands")(context)
            if not iterator:
                raise RuntimeError("Cannot allocate libkmod configuration iterator")
            try:
                while library.kmod_config_iter_next(iterator):
                    print(kind, os.fsdecode(library.kmod_config_iter_get_key(iterator)))
            finally:
                library.kmod_config_iter_free_iter(iterator)
    finally:
        library.kmod_unref(context)


if __name__ == "__main__":
    if len(sys.argv) < 4:
        sys.exit(
            "Usage: fde-module-config.py <admitted-library> "
            "<module-directory> <config-path>..."
        )
    module_commands(*sys.argv[1:3], sys.argv[3:])
