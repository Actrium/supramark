/*
 * SupramarkMarkdownModule.java — TEST-ONLY host-JVM harness for the JNI bridge.
 *
 * This is NOT the production module (that one lives under
 * android/src/main/java/... and extends ReactContextBaseJavaModule). It is
 * a host stand-in that intentionally shares the exact fully-qualified name
 * com.supramark.markdownnative.SupramarkMarkdownModule, because JNI binds
 * native methods by mangled class name: the C symbols in
 * supramark_markdown_jni.c are
 * Java_com_supramark_markdownnative_SupramarkMarkdownModule_native*.
 *
 * It declares the same two static native methods and exercises the C JNI
 * marshalling layer (byte[] <-> C bytes <-> FFI, status out-param, UTF-8
 * round-trip) on a plain host JVM via scripts/test-android-jni.sh — no
 * Android NDK, emulator, gradle or React Native needed. It does not cover
 * the production Java Promise/Executor wrapper, only the C bridge.
 */
package com.supramark.markdownnative;

import java.nio.charset.StandardCharsets;

public final class SupramarkMarkdownModule {
    // Status codes mirror the C ABI (SUPRAMARK_MARKDOWN_*).
    static final int OK = 0;
    static final int ERR_SERIALIZE = 1;
    static final int ERR_NULL_INPUT = 2;

    static native byte[] nativeParseJson(byte[] sourceUtf8, int[] statusOut);
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
            System.err.println("usage: SupramarkMarkdownModule <path-to-jni-shared-lib>");
            System.exit(2);
        }
        System.load(args[0]);

        // 1. A normal document round-trips to AST v2 JSON with a root node.
        int[] st = { -1 };
        byte[] out = nativeParseJson("# Hello".getBytes(StandardCharsets.UTF_8), st);
        check(st[0] == OK, "simple: status OK");
        check(out != null, "simple: bytes non-null");
        String json = out == null ? "" : new String(out, StandardCharsets.UTF_8);
        check(json.contains("\"type\"") && json.contains("root"), "simple: JSON has root node");

        // 2. Empty byte[] (length 0) is a valid empty document, not an error.
        //    Scope note: this guards bridge-level behaviour. It does not, by
        //    itself, demonstrate a pre-fix failure: empirically the pre-fix
        //    strlen path reads GetByteArrayElements' empty-array pointer as an
        //    empty string and resolves the same way — on a host JVM AND on a
        //    real Android 15 arm64 ART emulator (the UB is benign on both).
        //    The input_len == 0 fix is therefore a UB-hygiene + NULL-handling
        //    correctness change; its teeth live in the Rust suite, where
        //    parse_empty_input_null_ptr passes an explicit NULL pointer that
        //    the pre-fix code rejected with ERR_NULL_INPUT and the fix accepts
        //    as an empty document.
        st[0] = -1;
        byte[] outEmpty = nativeParseJson(new byte[0], st);
        check(st[0] == OK, "empty: status OK (input_len==0 is empty document)");
        check(outEmpty != null, "empty: bytes non-null");
        String jsonEmpty = outEmpty == null ? "" : new String(outEmpty, StandardCharsets.UTF_8);
        check(jsonEmpty.contains("\"type\"") && jsonEmpty.contains("root"), "empty: JSON has root node");

        // 3. Null source array -> null result, status reports NULL_INPUT.
        st[0] = -1;
        byte[] outNull = nativeParseJson(null, st);
        check(outNull == null, "null: result is null");
        check(st[0] == ERR_NULL_INPUT, "null: status ERR_NULL_INPUT");

        // 4. Invalid UTF-8 bytes -> null result, status NULL_INPUT.
        st[0] = -1;
        byte[] outBad = nativeParseJson(new byte[] { 0x23, (byte) 0xFF, (byte) 0xFE }, st);
        check(outBad == null, "invalid-utf8: result is null");
        check(st[0] == ERR_NULL_INPUT, "invalid-utf8: status ERR_NULL_INPUT");

        // 5. Version string marshals back as a non-empty Java String.
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
