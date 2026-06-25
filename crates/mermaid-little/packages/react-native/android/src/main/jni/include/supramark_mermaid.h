/*
 * supramark_mermaid.h — C ABI for the `supramark-mermaid-native`
 * Rust library (a wrapper around `mermaid-little`).
 *
 * Host integration:
 *
 *   1. Link `libsupramark_mermaid_native.a` (static) or
 *      `libsupramark_mermaid_native.{so,dylib,dll}` (shared).
 *   2. Install a text-measurement callback once at module init via
 *      `supramark_install_metrics_callback` (exported by the
 *      `font-metrics` crate; declare it locally — see README).
 *   3. Call `supramark_mermaid_render` / `..._with_id` to convert
 *      Mermaid source to SVG. The returned buffer is owned by the
 *      caller; release with `supramark_mermaid_free` when done.
 *
 * Thread safety: every entry point is safe to call from any thread
 * once the metrics callback has been installed.
 *
 * ABI stability: the error-code values and function signatures
 * declared in this header are part of the binary contract. Adding
 * new codes / functions is allowed; renumbering or repurposing
 * existing ones is not.
 */
#ifndef SUPRAMARK_MERMAID_H
#define SUPRAMARK_MERMAID_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* ── error codes ─────────────────────────────────────────────────── */

/* Success. `out_buf` / `out_len` point at a freshly-allocated SVG. */
#define SUPRAMARK_MERMAID_OK              0

/* Mermaid source could not be parsed (syntax / unknown diagram /
 * config error). The render half of the pipeline was not entered. */
#define SUPRAMARK_MERMAID_ERR_PARSE       1

/* Parser succeeded but layout / SVG emission failed. Also returned
 * if a Rust panic was caught at the ABI boundary. */
#define SUPRAMARK_MERMAID_ERR_RENDER      2

/* A required pointer argument was null, or input/id length was
 * non-zero with a null pointer. No allocation took place; out_buf
 * is left as (NULL, 0). */
#define SUPRAMARK_MERMAID_ERR_NULL_INPUT  3

/* ── functions ───────────────────────────────────────────────────── */

/* Render Mermaid source `input` (UTF-8, length `input_len`, NOT
 * required to be NUL-terminated) into a freshly-allocated SVG
 * buffer.
 *
 * On success: returns SUPRAMARK_MERMAID_OK and writes the buffer
 * pointer + byte length to *out_buf / *out_len. The caller owns
 * the buffer and MUST hand it back to `supramark_mermaid_free`.
 *
 * On error: returns a non-zero code and writes (NULL, 0) to the
 * out parameters. No allocation needs to be freed. */
int32_t supramark_mermaid_render(
    const uint8_t *input,
    size_t         input_len,
    uint8_t      **out_buf,
    size_t        *out_len);

/* Same as `supramark_mermaid_render` but with an explicit diagram
 * id, mirroring upstream Mermaid's `mermaid.render(id, src)`
 * signature. Useful for stable element ids in hash-keyed caches
 * or DOM-targeted re-renders. `id` may be NULL when `id_len == 0`. */
int32_t supramark_mermaid_render_with_id(
    const uint8_t *input,
    size_t         input_len,
    const uint8_t *id,
    size_t         id_len,
    uint8_t      **out_buf,
    size_t        *out_len);

/* Release a buffer previously returned by `supramark_mermaid_render*`.
 *
 * Passing (NULL, 0) is a no-op so callers may invoke this
 * unconditionally on the error path.
 *
 * Passing any other (pointer, length) combination — including one
 * produced by a different allocator, a pointer that has already
 * been freed, or a length that does not match what the render call
 * returned — is undefined behaviour. */
void supramark_mermaid_free(uint8_t *buf, size_t len);

/* Return a NUL-terminated static string with the crate's compile-
 * time version (e.g. "11.14.0-1"). The pointer is valid for the
 * lifetime of the loaded library; do not free it. */
const char *supramark_mermaid_version(void);

#ifdef __cplusplus
} /* extern "C" */
#endif

#endif /* SUPRAMARK_MERMAID_H */
