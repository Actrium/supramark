/* SPDX-License-Identifier: Apache-2.0 AND MIT */
/*
 * supramark_markdown.h — C ABI for libsupramark_markdown_native.{a,so,dylib}
 *
 * This header is the canonical contract between the native build
 * artefact and any C / Objective-C / Swift / Kotlin / JNI consumer.
 * The Rust impl lives in src/lib.rs; keep this file in sync with the
 * #[no_mangle] entry points and SUPRAMARK_MARKDOWN_* constants there.
 */

#ifndef SUPRAMARK_MARKDOWN_H
#define SUPRAMARK_MARKDOWN_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* ---- Error codes (must match SUPRAMARK_MARKDOWN_* in src/lib.rs) ---- */

/* Parse succeeded; *out_buf / *out_len populated, caller owns buffer. */
#define SUPRAMARK_MARKDOWN_OK               0
/* AST serialization to JSON failed. */
#define SUPRAMARK_MARKDOWN_ERR_SERIALIZE    1
/* A required pointer was NULL, or input was not valid UTF-8. */
#define SUPRAMARK_MARKDOWN_ERR_NULL_INPUT   2

/*
 * Sentinel input_len that opts into the NUL-terminated C-string path
 * (length computed via strlen). Every other value — including 0 — is an
 * explicit byte length, so input_len == 0 is a valid empty document.
 */
#define SUPRAMARK_MARKDOWN_LEN_CSTRING      ((size_t)-1)

/*
 * Parse a Markdown source string into AST v2 JSON.
 *
 * input      : pointer to the Markdown source bytes. Pass the exact byte
 *              length in input_len (the buffer need NOT be
 *              NUL-terminated). For an empty document pass input_len = 0;
 *              `input` is then not dereferenced and may be NULL.
 * input_len  : number of bytes at `input`. 0 means an empty document.
 *              Pass SUPRAMARK_MARKDOWN_LEN_CSTRING to treat `input` as a
 *              NUL-terminated C string (length via strlen).
 * out_buf    : on success, *out_buf is set to a heap-allocated UTF-8
 *              JSON byte buffer. NOT NUL-terminated. Caller MUST
 *              release with supramark_markdown_free.
 * out_len    : on success, *out_len is set to the byte length of the
 *              buffer at *out_buf.
 *
 * The returned JSON matches the schema produced by the wasm-bindgen
 * wrapper `@supramark/markdown-web`'s `parse_json`, so JS consumers
 * can `JSON.parse` it identically across Web / Node / RN runtimes.
 *
 * Returns one of the SUPRAMARK_MARKDOWN_* status codes. On non-OK
 * return the out-params are zero-initialised (NULL / 0).
 */
int32_t supramark_markdown_parse_json(
    const char* input, size_t input_len,
    char** out_buf, size_t* out_len
);

/*
 * Release a buffer previously returned by supramark_markdown_parse_json.
 *
 * Passing (NULL, 0) is a no-op. Passing a buffer that did not come
 * from supramark_markdown_parse_json, or a `len` that does not match
 * the original allocation, is undefined behaviour: the buffer is freed
 * through Rust's global allocator and may not match the C runtime's
 * free(3).
 */
void supramark_markdown_free(char* buf, size_t len);

/*
 * Returns a static, NUL-terminated UTF-8 version string identifying
 * this build of libsupramark_markdown_native. The returned pointer is
 * valid for the lifetime of the loaded library; do NOT free it.
 */
const char* supramark_markdown_version(void);

#ifdef __cplusplus
} /* extern "C" */
#endif

#endif /* SUPRAMARK_MARKDOWN_H */
