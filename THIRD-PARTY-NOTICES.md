# Third-party notices

nativelite-xkb is written from scratch and has no dependencies. It contains
no code from other projects. It does contain facts and test data that come
from them; this file says which, and carries their notices.

## Keysym names and numbers: the X11 keysym definitions

- **What is used:** the name and number of each keysym, and nothing else (no
  comments, no code), in `src/keysym_table.rs`. They were read from the
  `#define XKB_KEY_<name> <number>` lines of libxkbcommon 1.6.0's
  `include/xkbcommon/xkbcommon-keysyms.h`, which is generated from the X11
  protocol's `keysymdef.h` and `XF86keysym.h`, by `tools/gen_keysyms.py`.
- **Source:** https://github.com/xkbcommon/libxkbcommon (tag `xkbcommon-1.6.0`),
  https://gitlab.freedesktop.org/xorg/proto/xorgproto
- **Licence:** MIT/X11-style. The notice of the keysym definitions:

```
Copyright 1987, 1994, 1998  The Open Group

Permission to use, copy, modify, distribute, and sell this software and its
documentation for any purpose is hereby granted without fee, provided that
the above copyright notice appear in all copies and that both that
copyright notice and this permission notice appear in supporting
documentation.

The above copyright notice and this permission notice shall be included
in all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS
OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
IN NO EVENT SHALL THE OPEN GROUP BE LIABLE FOR ANY CLAIM, DAMAGES OR
OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE,
ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR
OTHER DEALINGS IN THE SOFTWARE.

Except as contained in this notice, the name of The Open Group shall
not be used in advertising or otherwise to promote the sale, use or
other dealings in this Software without prior written authorization
from The Open Group.


Copyright 1987 by Digital Equipment Corporation, Maynard, Massachusetts

                        All Rights Reserved

Permission to use, copy, modify, and distribute this software and its
documentation for any purpose and without fee is hereby granted,
provided that the above copyright notice appear in all copies and that
both that copyright notice and this permission notice appear in
supporting documentation, and that the name of Digital not be
used in advertising or publicity pertaining to distribution of the
software without specific, written prior permission.

DIGITAL DISCLAIMS ALL WARRANTIES WITH REGARD TO THIS SOFTWARE, INCLUDING
ALL IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS, IN NO EVENT SHALL
DIGITAL BE LIABLE FOR ANY SPECIAL, INDIRECT OR CONSEQUENTIAL DAMAGES OR
ANY DAMAGES WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS,
WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION,
ARISING OUT OF OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THIS
SOFTWARE.
```

## Keysym to Unicode mapping, canonical names: libxkbcommon

- **What is used:** for each keysym, the Unicode code point it stands for
  and which of its names is the canonical one, as numbers and names in
  `src/keysym_table.rs`. They were obtained by `tools/gen_keysyms.py` asking
  the installed libxkbcommon 1.6.0 for every keysym. No code was taken.
  (libxkbcommon's own table for this mapping, `src/keysym-utf.c`, is by
  Markus G. Kuhn and others and is in the public domain according to that
  file.)
- **Source:** https://github.com/xkbcommon/libxkbcommon
- **Licence:** MIT. The notice of the library's own code:

```
Copyright © 2009-2012, 2016 Daniel Stone
Copyright © 2012 Ran Benita <ran234@gmail.com>
Copyright © 2010, 2012 Intel Corporation
Copyright © 2008, 2009 Dan Nicholson
Copyright © 2010 Francisco Jerez <currojerez@riseup.net>

Permission is hereby granted, free of charge, to any person obtaining a
copy of this software and associated documentation files (the "Software"),
to deal in the Software without restriction, including without limitation
the rights to use, copy, modify, merge, publish, distribute, sublicense,
and/or sell copies of the Software, and to permit persons to whom the
Software is furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice (including the next
paragraph) shall be included in all copies or substantial portions of the
Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.  IN NO EVENT SHALL
THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
DEALINGS IN THE SOFTWARE.
```

libxkbcommon is also the reference this library is tested against
(`tests/differential.rs` loads the system's copy at run time; it is not
linked, shipped or required).

## Test fixtures: xkeyboard-config

- **What is used:** `tests/fixtures/us.xkb`, `de.xkb` and `us_ru.xkb` are
  complete keymaps for those layouts, as libxkbcommon 1.6.0 compiles them
  from the xkeyboard-config 2.41 data files (Ubuntu's `xkb-data` package).
  They are test data only: they are not part of the library and not in the
  published crate's code.
- **Source:** https://gitlab.freedesktop.org/xkeyboard-config/xkeyboard-config
- **Licence:** MIT/X11-style, several notices. They are reproduced in full in
  `tests/fixtures/COPYING.xkeyboard-config` (the copyright file of the
  `xkb-data` package the fixtures were made from).
