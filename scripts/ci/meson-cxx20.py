"""Meson wrapper that builds C++ as C++20.

MSVC rejects designated initializers in C++17. WebRTC's bundled Meson
project defaults to c++17, and the Rust build script does not override it.
"""
import os
import subprocess
import sys

real = os.environ["HUSH_MESON"]
args = sys.argv[1:]
if args and args[0] == "setup" and not any(arg.startswith("-Dcpp_std=") for arg in args):
    args.insert(1, "-Dcpp_std=c++20")
raise SystemExit(subprocess.call([real, *args]))
