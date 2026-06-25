/*
 * SupramarkMermaidModule.java — TEST-ONLY host-JVM harness for the JNI bridge.
 *
 * This is NOT the production module (that one lives under
 * android/src/main/java/... and extends ReactContextBaseJavaModule). It is
 * a host stand-in that intentionally shares the exact fully-qualified name
 * com.supramark.mermaidnative.SupramarkMermaidModule, because JNI binds
 * native methods by mangled class name: the C symbols in
 * supramark_mermaid_jni.c are
 * Java_com_supramark_mermaidnative_SupramarkMermaidModule_native*.
 *
 * It declares the same two static native methods and exercises the C JNI
 * marshalling layer (String <-> C UTF-8 <-> FFI, status out-param, SVG
 * round-trip) on a plain host JVM via scripts/test-android-jni.sh — no
 * Android NDK, emulator, gradle or React Native needed. It does not cover
 * the production Java Promise/Executor wrapper, only the C bridge.
 */
package com.supramark.mermaidnative;

public final class SupramarkMermaidModule {
    // Status codes mirror the C ABI (SUPRAMARK_MERMAID_*).
    static final int OK             = 0;
    static final int ERR_PARSE      = 1;
    static final int ERR_RENDER     = 2;
    static final int ERR_NULL_INPUT = 3;

    static native String nativeRender(String source, int[] statusOut);
    static native String nativeGetVersion();

    private static int failures = 0;

    private static void check(boolean cond, String msg) {
        System.out.println((cond ? "ok   - " : "FAIL - ") + msg);
        if (!cond) {
            failures++;
        }
    }

    public static void main(String[] args) {
        if (args.length < 1) {
            System.err.println("usage: SupramarkMermaidModule <path-to-jni-shared-lib>");
            System.exit(2);
        }
        System.load(args[0]);

        // 1. A real Mermaid diagram renders to an SVG document, status OK.
        int[] st = { -1 };
        String svg = nativeRender("graph TD; A-->B", st);
        check(st[0] == OK, "diagram: status OK");
        check(svg != null, "diagram: result non-null");
        check(svg != null && svg.contains("<svg"), "diagram: output contains <svg");

        // 2. Empty source. input_len comes from GetStringUTFLength, so an
        //    empty string is a zero-length (not NULL) Mermaid source. It must
        //    not crash and must report a definite status — never silently
        //    corrupt the out-param.
        st[0] = -1;
        String svgEmpty = nativeRender("", st);
        check(st[0] == OK || st[0] == ERR_PARSE || st[0] == ERR_RENDER,
              "empty: status is a defined code (" + st[0] + ")");
        check((st[0] == OK) == (svgEmpty != null),
              "empty: result non-null iff status OK");

        // 3. Null source -> null result, status reports NULL_INPUT (the C
        //    bridge short-circuits before touching the FFI).
        st[0] = -1;
        String svgNull = nativeRender(null, st);
        check(svgNull == null, "null: result is null");
        check(st[0] == ERR_NULL_INPUT, "null: status ERR_NULL_INPUT");

        // 4. Version string marshals back as a non-empty Java String.
        String version = nativeGetVersion();
        check(version != null && !version.isEmpty(), "version: non-empty string");

        if (failures == 0) {
            System.out.println("\nALL PASSED");
            System.exit(0);
        } else {
            System.out.println("\n" + failures + " CHECK(S) FAILED");
            System.exit(1);
        }
    }
}
