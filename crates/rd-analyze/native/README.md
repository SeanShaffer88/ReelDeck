# Vendored MediaKit binaries

These binaries are vendored so ReelDeck can link MediaKit at build time
without a network fetch or a local MediaKit checkout.

- Product: MediaKit v0.1.0 (MIT)
- Source: https://github.com/SeanShaffer88/MediaKit/releases/download/v0.1.0/MediaKit-v0.1.0-x64.zip
- Release zip SHA-256: `85b14a7cf691e1962c6db046b47a95c18eab73db8992cb12a0350c63f0d045e9`
- `media_kit.dll` SHA-256: `830dbeb7d3acc4e356001a809baa40482b5825f81fa59eb8bcbdd0bd94cbef21`
- `media_kit.dll.lib` SHA-256: `6132f5f5a4a30d69bf6af1ed8dfe5edf8996b6777adb81f3b9f4006ca021d1c8`

The C header in the release zip is byte-identical in content to
MediaKit's `include/mediakit.h` at v0.1.0 (line endings aside); the FFI
declarations in the `rd-mediakit` crate are written against that header.

Linking and runtime notes (Windows):

- `rd-mediakit`'s build script links the import library
  (`media_kit.dll.lib`, staged under the canonical `media_kit.lib` name
  for the linker) — no MediaKit source is compiled into ReelDeck.
- At run time Windows loads `media_kit.dll` from beside the executable.
  Ship `media_kit.dll` next to `reeldeck.exe`. For local test runs,
  copy it into `target/debug/deps` (and `target/debug`) so the test
  binaries can start; CI does this in its workflow.
- If the DLL is absent at run time, Windows refuses to start the
  executable. That is a deployment requirement, not a fallback case:
  the "MediaKit unavailable" fallback inside ReelDeck covers a
  *running* program whose MediaKit call reports an error, a container
  MediaKit does not recognize, and non-Windows platforms (where the
  probe is not compiled in at all).
