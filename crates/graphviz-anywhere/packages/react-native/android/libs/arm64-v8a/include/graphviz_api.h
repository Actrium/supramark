/*
 * graphviz-anywhere C ABI wrapper
 *
 * Provides a simplified, stable C interface to Graphviz core functionality.
 * Designed for easy FFI consumption from Rust, React Native, and other languages.
 *
 * Licensed under the Apache License, Version 2.0
 */

#ifndef GRAPHVIZ_API_H
#define GRAPHVIZ_API_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#ifdef _WIN32
#ifdef GRAPHVIZ_API_EXPORTS
#define GV_API __declspec(dllexport)
#else
#define GV_API __declspec(dllimport)
#endif
#else
#define GV_API __attribute__((visibility("default")))
#endif

/* Error codes */
typedef enum {
    GV_OK = 0,
    GV_ERR_NULL_INPUT = -1,
    GV_ERR_INVALID_DOT = -2,
    GV_ERR_LAYOUT_FAILED = -3,
    GV_ERR_RENDER_FAILED = -4,
    GV_ERR_INVALID_ENGINE = -5,
    GV_ERR_INVALID_FORMAT = -6,
    GV_ERR_OUT_OF_MEMORY = -7,
    GV_ERR_NOT_INITIALIZED = -8,
} gv_error_t;

/* Opaque context handle */
typedef struct gv_context gv_context_t;

/*
 * Create a new Graphviz context.
 * Returns NULL on failure.
 * The caller must free the context with gv_context_free().
 */
GV_API gv_context_t *gv_context_new(void);

/*
 * Free a Graphviz context and all associated resources.
 */
GV_API void gv_context_free(gv_context_t *ctx);

/*
 * Render a DOT string to the specified format using the given layout engine.
 *
 * Parameters:
 *   ctx        - Graphviz context (created with gv_context_new)
 *   dot        - DOT language input string (null-terminated)
 *   engine     - Layout engine name: "dot", "neato", "fdp", "sfdp", "circo", "twopi", "osage", "patchwork"
 *   format     - Output format: "svg", "png", "pdf", "ps", "json", "dot", "xdot", "plain"
 *   out_data   - Pointer to receive the output data buffer (caller must free with gv_free_render_data)
 *   out_length - Pointer to receive the output data length in bytes
 *
 * Returns GV_OK on success, or a negative error code.
 */
GV_API gv_error_t gv_render(gv_context_t *ctx,
                             const char *dot,
                             const char *engine,
                             const char *format,
                             char **out_data,
                             size_t *out_length);

/*
 * Free render output data returned by gv_render().
 */
GV_API void gv_free_render_data(char *data);

/*
 * Get a human-readable description of an error code.
 */
GV_API const char *gv_strerror(gv_error_t err);

/*
 * Get the Graphviz library version string.
 */
GV_API const char *gv_version(void);

/*
 * Get the list of available layout engines as a JSON array string.
 * Returns a pointer to a static string (must not be freed).
 */
GV_API const char *gv_get_engines(void);

/*
 * Get the list of available output formats as a JSON array string.
 * Returns a pointer to a static string (must not be freed).
 */
GV_API const char *gv_get_formats(void);

/*
 * Render a DOT string to multiple formats in a single call.
 *
 * Parameters:
 *   ctx        - Graphviz context (created with gv_context_new)
 *   dot        - DOT language input string (null-terminated)
 *   engine     - Layout engine name
 *   formats    - JSON array of format strings (e.g., '["svg","png","pdf"]')
 *   out_data   - Pointer to receive JSON object with format->output mapping
 *   out_length - Pointer to receive the output data length in bytes
 *
 * Returns GV_OK on success, or a negative error code.
 */
GV_API gv_error_t gv_render_formats(gv_context_t *ctx,
                                     const char *dot,
                                     const char *engine,
                                     const char *formats,
                                     char **out_data,
                                     size_t *out_length);

#ifdef __cplusplus
}
#endif

#endif /* GRAPHVIZ_API_H */
