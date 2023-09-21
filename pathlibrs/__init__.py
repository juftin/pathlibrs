"""
pathlib.rs: A pathlib.Path implementation, written in Rust.
"""

from os import PathLike

from .pathlibrs import Path

PathLike.register(Path)
__all__ = ["Path"]
