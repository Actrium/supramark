/*
 * SupramarkMarkdownModule.java — RN bridge module for supramark-markdown native FFI.
 *
 * Loads libsupramark_markdown_jni.so (the JNI shim, which in turn links
 * libsupramark_markdown_native.so), dispatches parse calls off the JS
 * thread, and resolves promises with the produced AST v2 JSON string.
 */

package com.supramark.markdownnative;

import androidx.annotation.NonNull;

import com.facebook.react.bridge.Promise;
import com.facebook.react.bridge.ReactApplicationContext;
import com.facebook.react.bridge.ReactContextBaseJavaModule;
import com.facebook.react.bridge.ReactMethod;

import java.nio.charset.StandardCharsets;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;

public class SupramarkMarkdownModule extends ReactContextBaseJavaModule {

    public static final String NAME = "SupramarkMarkdownNative";

    private static final int OK             = 0;
    private static final int ERR_SERIALIZE  = 1;
    private static final int ERR_NULL_INPUT = 2;

    static {
        // Loading the JNI shim transitively pulls in libsupramark_markdown_native.so
        // via its DT_NEEDED entry (set up by CMake's IMPORTED target).
        System.loadLibrary("supramark_markdown_jni");
    }

    private final ExecutorService parseQueue =
        Executors.newSingleThreadExecutor(r -> {
            Thread t = new Thread(r, "supramark-markdown-native-parse");
            t.setDaemon(true);
            return t;
        });

    public SupramarkMarkdownModule(ReactApplicationContext reactContext) {
        super(reactContext);
    }

    @Override
    @NonNull
    public String getName() {
        return NAME;
    }

    // Pass UTF-8 bytes through JNI so Rust receives standard UTF-8, not JNI modified UTF-8.
    private static native byte[] nativeParseJson(byte[] sourceUtf8, int[] statusOut);
    private static native String nativeGetVersion();

    @ReactMethod
    public void parseJson(final String source, final Promise promise) {
        if (source == null) {
            promise.reject("NULL_INPUT", "parseJson: source is null");
            return;
        }
        parseQueue.execute(() -> {
            try {
                int[] status = new int[]{ ERR_SERIALIZE };
                byte[] jsonUtf8 = nativeParseJson(source.getBytes(StandardCharsets.UTF_8), status);
                // Null bytes mean the native parser failed before producing JSON.
                if (jsonUtf8 == null || status[0] != OK) {
                    String code;
                    switch (status[0]) {
                        case ERR_SERIALIZE:  code = "SERIALIZE_ERROR"; break;
                        case ERR_NULL_INPUT: code = "NULL_INPUT"; break;
                        default:             code = "UNKNOWN"; break;
                    }
                    promise.reject(code, "supramark_markdown_parse_json returned " + status[0]);
                    return;
                }
                String json = new String(jsonUtf8, StandardCharsets.UTF_8);
                promise.resolve(json);
            } catch (Throwable t) {
                promise.reject("UNKNOWN", t.toString(), t);
            }
        });
    }

    @ReactMethod
    public void getVersion(final Promise promise) {
        try {
            String v = nativeGetVersion();
            if (v == null) {
                promise.reject("UNKNOWN", "supramark_markdown_version returned NULL");
            } else {
                promise.resolve(v);
            }
        } catch (Throwable t) {
            promise.reject("UNKNOWN", t.toString(), t);
        }
    }
}
