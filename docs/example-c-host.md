# Example: a host in 40 lines of C

The smallest useful host: compile a `.slint` file at run time, draw it on the CPU, set a property,
click a button and read the result back. It shows the order of calls described in
[frame-lifecycle.md](frame-lifecycle.md) without a window. It was compiled and run on macOS
(Apple Silicon) against the committed header, `swift/Sources/CSlintBindings/slint_bindings.h`.

## Build the library

The FFI crate is an `rlib`; ask for the static library explicitly, from the repository root:

```bash
cargo rustc -p slint-bindings-ffi --lib --crate-type staticlib
# -> target/debug/libslint_bindings_ffi.a
```

## The program

`demo.c` drives `crates/slint-bindings-core/ui/demo.slint`, a form with a name field, a Continue
button (centre at 80, 112 logical px), a `name` property, a `submit-count` property and a
`submitted(string)` callback:

```c
#include <stdio.h>
#include <stdlib.h>
#include "slint_bindings.h"

static void on_submitted(void *user_data, const char *args_json) {
  printf("submitted(%s)\n", args_json);
  *(int *)user_data += 1;
}

int main(void) {
  /* Compile the form from disk (a null source reads the file at `path`). */
  struct SbDefinition *def = sb_compile(NULL, "crates/slint-bindings-core/ui/demo.slint");
  if (!def) { fprintf(stderr, "compile: %s\n", sb_last_error()); return 1; }
  struct SbHost *host = sb_component_new(def, 320, 200, 1.0f); /* CPU-drawn, 320x200 px */
  sb_definition_free(def);                                      /* the host stays valid */
  if (!host) { fprintf(stderr, "new: %s\n", sb_last_error()); return 1; }

  int calls = 0;
  sb_on_callback(host, "submitted", on_submitted, &calls);
  sb_set_property_json(host, "name", "\"Ada\"");

  sb_tick();                                   /* advance Slint's timers and animations */
  size_t len = sb_host_frame_len(host);
  uint8_t *buf = malloc(len);
  struct SbFrame f = sb_host_render(host, buf, len);           /* RGBA8 */
  printf("first frame: redrawn=%d failed=%d bytes=%zu\n", f.redrawn, f.failed, len);

  /* A click on the Continue button. */
  sb_host_pointer_pressed(host, 80, 112, SB_POINTER_BUTTON_LEFT);
  sb_host_pointer_released(host, 80, 112, SB_POINTER_BUTTON_LEFT);
  sb_tick();

  char *count = sb_get_property_json(host, "submit-count");
  printf("submit-count = %s, callback calls = %d\n", count, calls);
  sb_string_free(count);

  free(buf);
  sb_host_free(host);
  return 0;
}
```

## Link and run

```bash
cc -I swift/Sources/CSlintBindings demo.c target/debug/libslint_bindings_ffi.a \
   -framework Foundation -framework AppKit -framework CoreFoundation -framework CoreGraphics \
   -framework CoreText -framework Metal -framework QuartzCore -framework IOKit \
   -framework Security -framework SystemConfiguration -liconv -lc++ -o demo
./demo
```

```
first frame: redrawn=1 failed=0 bytes=256000
submitted(["Ada"])
submit-count = 1, callback calls = 1
```

`256000` is 320 × 200 × 4 bytes. Properties and callback arguments cross as JSON
(`sb_set_property_json`, `sb_get_property_json`, `sb_on_callback`); every call must come from the
thread that created the host.

## Where next

- A real window: [frame-lifecycle.md](frame-lifecycle.md) for the display-link loop and the Metal
  path (`sb_component_new_metal`).
- Windows: [../windows/README.md](../windows/README.md).
- Every function: the header comments in `slint_bindings.h`.
