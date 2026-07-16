# Notes on Reproducible Builds

> SYNTHETIC SAMPLE — hand-authored for the streaming golden test. Exercises Markdown **footnotes** (`[^label]` references with matching definitions), which were absent from the real session corpus.

A reproducible build is one where the same source, given the same declared environment, yields a bit-for-bit identical artifact.[^determinism] This is stronger than "it compiles on my machine" and weaker than "it runs everywhere" — it is specifically about the *output bytes*.

## Sources of non-determinism

Most irreproducibility comes from a short list of usual suspects:

1. **Timestamps** baked into archives or object files.[^time]
2. **Filesystem ordering** — globbing returns entries in inode order on some systems.[^order]
3. **Absolute paths** embedded in debug info.
4. **Locale and timezone** leaking into sorted output or formatted dates.

Each has a standard mitigation. For timestamps, honor the `SOURCE_DATE_EPOCH` environment variable, which downstream tooling reads to clamp every embedded date.[^time] For ordering, sort explicitly before feeding a file list into an archiver rather than trusting the shell.

## A minimal check

```bash
# Build twice into separate trees, then compare.
build --out /tmp/a
build --out /tmp/b
diff -r /tmp/a /tmp/b && echo "reproducible"
```

If the trees differ, `diffoscope` will tell you *which* bytes moved — it recurses into archives, ELF sections, and even PNG chunks.[^diffoscope]

## Why it matters

Reproducibility is the backbone of supply-chain trust: if two independent parties can rebuild the published binary and get the same hash, a compromised build server can no longer smuggle in a payload undetected.[^determinism] It also makes caching sound — a content-addressed cache is only correct if identical inputs truly produce identical outputs.

The rendering invariant this fixture guards: a streaming split must not separate a footnote *reference* from the tail that eventually carries its *definition*. If the segmenter breaks `[^time]` into one segment and `[^time]:` into another, the per-segment HTML will render dangling superscripts that never appear in the whole-document render.

[^determinism]: Determinism here means a pure function from (source, environment) to bytes — no hidden inputs like wall-clock time or random seeds.
[^time]: See the `SOURCE_DATE_EPOCH` specification; setting it to a fixed integer clamps embedded modification times across most build tools.
[^order]: Readdir order is not guaranteed by POSIX; always sort a directory listing before it influences output.
[^diffoscope]: `diffoscope` is a tool that produces human-readable, recursive diffs of binary artifacts and container formats.
