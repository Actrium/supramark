/* supramark_plantuml.h — C ABI for the supramark-plantuml-native crate.
 *
 * Mirror of the public surface exposed by
 * crates/plantuml-little/packages/native/src/lib.rs. Keep this header in
 * sync with the Rust side; the Rust doc-comments are the source of
 * truth for the contract, this file is the consumer-facing summary.
 *
 * SPDX-License-Identifier: GPL-3.0-or-later OR LGPL-3.0-or-later OR Apache-2.0 OR EPL-2.0 OR MIT
 */

#ifndef SUPRAMARK_PLANTUML_H
#define SUPRAMARK_PLANTUML_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* ── Error codes ────────────────────────────────────────────────── */

#define SUPRAMARK_PLANTUML_OK              0
#define SUPRAMARK_PLANTUML_ERR_PARSE       1  /* input not valid UTF-8 */
#define SUPRAMARK_PLANTUML_ERR_RENDER      2  /* plantuml-little returned Err */
#define SUPRAMARK_PLANTUML_ERR_NULL_INPUT  3  /* required pointer was null */

/* ── Render API ─────────────────────────────────────────────────── */

/* Convert a UTF-8 PlantUML source buffer to an SVG buffer.
 *
 * On SUPRAMARK_PLANTUML_OK:
 *   *out_buf — heap-allocated UTF-8 bytes (NOT NUL-terminated).
 *              Must be released with supramark_plantuml_free.
 *   *out_len — byte length of the buffer.
 *
 * On any error code: *out_buf is set to NULL and *out_len to 0
 * (when those pointers are themselves non-null).
 *
 * Thread-safe; the host's metrics callback (installed via
 * supramark_install_metrics_callback) must therefore also be
 * thread-safe.
 */
int supramark_plantuml_render(const char *input,
                              size_t input_len,
                              uint8_t **out_buf,
                              size_t *out_len);

/* Free a buffer previously returned by supramark_plantuml_render.
 * No-op if buf is NULL. Calling with (buf, len) from any other source
 * is undefined behaviour — the Rust allocator and the host's
 * `free` are not interchangeable.
 */
void supramark_plantuml_free(uint8_t *buf, size_t len);

/* Pointer to a 'static NUL-terminated version string
 * (CARGO_PKG_VERSION of supramark-plantuml-native, which tracks
 * plantuml-little upstream). Do NOT free.
 */
const char *supramark_plantuml_version(void);

/* ── Metrics callback bridge (re-exported from font-metrics) ────── */

/* Host-supplied text-measurement function. `family` / `text` are raw
 * UTF-8 byte slices (ptr + length), NOT NUL-terminated. The callback
 * writes width / ascent / descent into the three out pointers; all
 * three are guaranteed non-null and writable for one double each by
 * the Rust caller.
 *
 * bold / italic use uint8_t (0 = false, non-zero = true) because the
 * C ABI representation of `_Bool` is implementation-defined on older
 * toolchains.
 */
typedef void (*supramark_measure_text_fn)(const char *family,
                                          size_t family_len,
                                          const char *text,
                                          size_t text_len,
                                          double size,
                                          uint8_t bold,
                                          uint8_t italic,
                                          double *out_width,
                                          double *out_ascent,
                                          double *out_descent);

/* Install the host-supplied measurement callback. Idempotent —
 * calling more than once replaces the previous installation
 * (last-write-wins). Typically called exactly once at host module
 * init, BEFORE any supramark_plantuml_render call.
 *
 * If the host never installs a callback (or installs one that
 * returns NaN / non-finite values), every measurement falls back to
 * a `size * 0.6`-per-char heuristic. The diagram still renders, just
 * with placeholder widths.
 */
void supramark_install_metrics_callback(supramark_measure_text_fn cb);

#ifdef __cplusplus
}
#endif

#endif /* SUPRAMARK_PLANTUML_H */
