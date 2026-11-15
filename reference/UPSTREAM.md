# Upstream build references

The project previously kept `fagci-Makefile` and `fagci-linker.ld` as research
references. No redistribution license was found in the upstream repository,
so these unused copies are not part of the public tree or source archive.

Source: [fagci/rt880-fw](https://github.com/fagci/rt880-fw).
Our Rust build uses `firmware-diagnostic/memory.x`, its own build script and
Cargo configuration. It does not require those upstream files.

Original copies remain in the owner's private pre-publication Git archive.
