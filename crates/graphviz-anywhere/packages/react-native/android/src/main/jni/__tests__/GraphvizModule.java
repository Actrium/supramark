/*
 * GraphvizModule.java — TEST-ONLY host-JVM harness for the JNI bridge.
 *
 * This is NOT the production module (that one lives under
 * android/src/main/java/... and extends ReactContextBaseJavaModule). It is
 * a host stand-in that intentionally shares the exact fully-qualified name
 * com.graphviznative.GraphvizModule, because JNI binds native methods by
 * mangled class name: the C symbols in graphviz_jni.c are
 * Java_com_graphviznative_GraphvizModule_native*.
 *
 * It declares the same static native methods and exercises the C JNI
 * marshalling layer (context lifecycle, String <-> C UTF-8 <-> the
 * graphviz_api C ABI, error-code return, String[] out-param) on a plain
 * host JVM via scripts/test-android-jni.sh — no Android NDK, emulator,
 * gradle or React Native needed. It does not cover the production Java
 * Promise/Executor wrapper, only the C bridge.
 */
package com.graphviznative;

public final class GraphvizModule {
    // Error codes mirror the gv_error_t enum in graphviz_api.h.
    static final int GV_OK             = 0;
    static final int GV_ERR_NULL_INPUT = -1;

    static native long nativeContextNew();
    static native void nativeContextFree(long ctx);
    static native int nativeRender(long ctx, String dot, String engine,
                                   String format, String[] outResult);
    static native String nativeStrerror(int err);
    static native String nativeVersion();

    private static int failures = 0;

    private static void check(boolean cond, String msg) {
        System.out.println((cond ? "ok   - " : "FAIL - ") + msg);
        if (!cond) {
            failures++;
        }
    }

    public static void main(String[] args) {
        if (args.length < 1) {
            System.err.println("usage: GraphvizModule <path-to-jni-shared-lib>");
            System.exit(2);
        }
        System.load(args[0]);

        // 0. A native context can be created (non-zero opaque pointer).
        long ctx = nativeContextNew();
        check(ctx != 0, "context: nativeContextNew returns non-zero handle");

        // 1. A tiny DOT graph renders to an SVG document via the "dot"
        //    engine, return code GV_OK, out-param[0] populated with markup.
        String[] out = new String[1];
        int err = nativeRender(ctx, "digraph{a->b}", "dot", "svg", out);
        check(err == GV_OK, "render: status GV_OK (got " + err + ")");
        check(out[0] != null, "render: out[0] non-null");
        check(out[0] != null && out[0].contains("<svg"), "render: output contains <svg");

        // 2. "json" is also a text format; the JNI returns it verbatim and
        //    it must carry a JSON object root.
        String[] outJson = new String[1];
        int errJson = nativeRender(ctx, "digraph{a->b}", "dot", "json", outJson);
        check(errJson == GV_OK, "render-json: status GV_OK (got " + errJson + ")");
        check(outJson[0] != null && outJson[0].contains("{"),
              "render-json: output is a JSON object");

        // 3. Invalid DOT source must surface a non-OK negative error code,
        //    not a crash, and must leave the out-param unset.
        String[] outBad = new String[1];
        int errBad = nativeRender(ctx, "this is not valid dot", "dot", "svg", outBad);
        check(errBad != GV_OK, "invalid-dot: status non-OK (got " + errBad + ")");
        check(outBad[0] == null, "invalid-dot: out[0] stays null");

        // 4. nativeStrerror maps an error code to a non-empty message.
        String msg = nativeStrerror(GV_ERR_NULL_INPUT);
        check(msg != null && !msg.isEmpty(), "strerror: non-empty message");

        // 5. Version string marshals back as a non-empty Java String.
        String version = nativeVersion();
        check(version != null && !version.isEmpty(), "version: non-empty string");

        // 6. Context can be released without crashing.
        nativeContextFree(ctx);
        check(true, "context: nativeContextFree completes");

        if (failures == 0) {
            System.out.println("\nALL PASSED");
            System.exit(0);
        } else {
            System.out.println("\n" + failures + " CHECK(S) FAILED");
            System.exit(1);
        }
    }
}
