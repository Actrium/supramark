/*
 * Minimal React Native header stub — TEST ONLY.
 *
 * GraphvizModule imports <React/RCTLog.h> and calls RCTLogError on one
 * path: when gv_context_new() fails inside -ensureContext. That path is
 * not exercised by the tests (the prebuilt Graphviz lib creates a
 * context successfully), but the macro must still be *defined* for the
 * module to compile without linking the full React Native framework.
 *
 * We forward RCTLogError to NSLog so any unexpected failure is still
 * visible in the xctest log, rather than silently dropped. This mirrors
 * the shape of the real macro (printf-style variadic) closely enough to
 * compile the module exactly as RN would. See RCTBridgeModule.h in this
 * directory for the broader rationale.
 */
#import <Foundation/Foundation.h>

#define RCTLogError(...)   NSLog(__VA_ARGS__)
#define RCTLogWarn(...)    NSLog(__VA_ARGS__)
#define RCTLogInfo(...)    NSLog(__VA_ARGS__)
#define RCTLog(...)        NSLog(__VA_ARGS__)
