# Third-party code and references

## baken — MIT License
https://github.com/M-Igashi/baken

`crates/pioneer-formats/src/pdb/fixed.rs` (default rows of the colors, columns,
menu and sort tables) is copied from baken, and the `export.pdb` page/row layout
rules in `crates/pioneer-formats/src/pdb/{writer,encode}.rs` follow its byte-level
measurements of rekordbox 7 exports.

```
MIT License

Copyright (c) 2025 Masanari Higashi

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

## rb2engine — MIT License
https://github.com/jrgutier/rb2engine

`crates/engine-library/ddl/schema_3_0_2.sql` (Engine schema captured from an
Engine DJ library) and the test fixtures `crates/engine-library/tests/fixtures/*.db`
come from rb2engine; the Engine blob layouts (`blobs.rs`), the beat grid
normalisation and the cue-to-pad policy (`convert.rs`) follow its code.

```
MIT License

Copyright (c) 2026 J.R. Gutierrez

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

## PenBridge — MIT License
https://github.com/athousanddetails/penbridge

`crates/engine-library/ddl/schema_2_21_0.sql` (Engine schema 2.21.0) comes from
PenBridge; the shared-`Contents` layout of a hybrid stick follows its approach.

```
MIT License

Copyright (c) 2026 A Thousand Details

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

## Format documentation
- Deep Symmetry — crate-digger / djl-analysis (Kaitai definitions, EPL-2.0 OR MPL-2.0 OR LGPL-3.0)
- libdjinterop (LGPL-3.0) and the Mixxx wiki — Engine library format, read as documentation only; no code taken
- rekordcrate (MPL-2.0) — used only as test fixtures and reference; the field offsets of the "My Settings" files (`crates/pioneer-formats/src/setting.rs`) follow its `setting.rs`, reimplemented
