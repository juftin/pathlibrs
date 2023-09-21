# pathlib.rs

A ⚡️fast⚡️ implementation of the [pathlib.Path](https://docs.python.org/3/library/pathlib.html) class.
Written in Rust, built for Python.

## Usage

```python
from pathlibrs import Path

p = Path("foo/bar")
p.mkdir(parents=True)
baz_file = p / "baz.txt"
baz_file.write_text("Hello, world!")
```

## Performance

`pathlibrs` is much faster than the standard library's `pathlib` module
for certain operations. For example, traversing a directory tree can be
~4x faster:

```python
from pathlib import Path
from pathlibrs import Path as PathRS

p = Path("~/Downloads")
r = PathRS("~/Downloads")

%timeit
list(p.glob("**/*"))
# 1.03 s ± 6.04 ms per loop (mean ± std. dev. of 7 runs, 1 loop each)

%timeit
list(r.glob("**/*"))
# 253 ms ± 1.04 ms per loop (mean ± std. dev. of 7 runs, 1 loop each)
```
