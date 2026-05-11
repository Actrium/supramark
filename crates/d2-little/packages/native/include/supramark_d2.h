/* SPDX-License-Identifier: MPL-2.0 */
/*
 * supramark_d2.h — C ABI for libsupramark_d2_native.{a,so,dylib}
 *
 * This header is the canonical contract between the native build
 * artefact and any C / Objective-C / Swift / Kotlin / JNI consumer.
 * The Rust impl lives in src/lib.rs; keep this file in sync with the
 * #[no_mangle] entry points and SUPRAMARK_D2_* constants there.
 *
 * Consumers also need to link `libfont_metrics.*` (statically merged
 * into libsupramark_d2_native.a in the default cargo build) and may
 * call supramark_install_metrics_callback from there to wire a
 * host-supplied measureText impl. See font-metrics's
 * src/ffi_callback.rs for that contract — it is intentionally NOT
 * re-declared here so there is exactly one source of truth per crate.
 */

#ifndef SUPRAMARK_D2_H
#define SUPRAMARK_D2_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* ---- Error codes (must match SUPRAMARK_D2_* in src/lib.rs) ---- */

/* Render succeeded; *out_buf / *out_len populated, caller owns buffer. */
#define SUPRAMARK_D2_OK                   0
/* Input failed to parse as D2 source. */
#define SUPRAMARK_D2_ERR_PARSE            1
/* Parsing succeeded but layout / SVG rendering raised an error. */
#define SUPRAMARK_D2_ERR_RENDER           2
/* A required pointer was NULL, or input was not valid UTF-8. */
#define SUPRAMARK_D2_ERR_NULL_INPUT       3

/*
 * Render a D2 source string to SVG.
 *
 * input      : pointer to the D2 source bytes. May be either a
 *              NUL-terminated C string (pass input_len = 0) or an
 *              explicit-length byte buffer (pass input_len > 0). The
 *              explicit-length form is preferred for large inputs.
 * input_len  : number of bytes at `input`, or 0 to use strlen-style.
 * out_buf    : on success, *out_buf is set to a heap-allocated UTF-8
 *              SVG byte buffer. NOT NUL-terminated. Caller MUST
 *              release with supramark_d2_free.
 * out_len    : on success, *out_len is set to the byte length of the
 *              buffer at *out_buf.
 *
 * Returns one of the SUPRAMARK_D2_* status codes. On non-OK return
 * the out-params are zero-initialised (NULL / 0).
 */
int32_t supramark_d2_render(
    const char* input, size_t input_len,
    char** out_buf, size_t* out_len
);

/*
 * Release a buffer previously returned by supramark_d2_render.
 *
 * Passing (NULL, 0) is a no-op. Passing a buffer that did not come
 * from supramark_d2_render, or a `len` that does not match the
 * original allocation, is undefined behaviour: the buffer is freed
 * through Rust's global allocator and may not match the C runtime's
 * free(3).
 */
void supramark_d2_free(char* buf, size_t len);

/*
 * Returns a static, NUL-terminated UTF-8 version string identifying
 * this build of libsupramark_d2_native. The returned pointer is
 * valid for the lifetime of the loaded library; do NOT free it.
 */
const char* supramark_d2_version(void);

#ifdef __cplusplus
} /* extern "C" */
#endif

#endif /* SUPRAMARK_D2_H */
