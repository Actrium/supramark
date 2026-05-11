/*
 * SupramarkMermaidModule.java — RN bridge module for d2 native FFI.
 *
 * Loads libsupramark_mermaid_jni.so (the JNI shim, which in turn links
 * libsupramark_mermaid_native.so), dispatches render calls off the JS
 * thread, and resolves promises with the produced SVG.
 */

package com.supramark.mermaidnative;

import androidx.annotation.NonNull;

import com.facebook.react.bridge.Promise;
import com.facebook.react.bridge.ReactApplicationContext;
import com.facebook.react.bridge.ReactContextBaseJavaModule;
import com.facebook.react.bridge.ReactMethod;

import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;

public class SupramarkMermaidModule extends ReactContextBaseJavaModule {

    public static final String NAME = "SupramarkMermaidNative";

    private static final int OK             = 0;
    private static final int ERR_PARSE      = 1;
    private static final int ERR_RENDER     = 2;
    private static final int ERR_NULL_INPUT = 3;

    static {
        // Loading the JNI shim transitively pulls in libsupramark_mermaid_native.so
        // via its DT_NEEDED entry (set up by CMake's IMPORTED target).
        System.loadLibrary("supramark_mermaid_jni");
    }

    private final ExecutorService renderQueue =
        Executors.newSingleThreadExecutor(r -> {
            Thread t = new Thread(r, "supramark-mermaid-native-render");
            t.setDaemon(true);
            return t;
        });

    public SupramarkMermaidModule(ReactApplicationContext reactContext) {
        super(reactContext);
    }

    @Override
    @NonNull
    public String getName() {
        return NAME;
    }

    private static native String nativeRender(String source, int[] statusOut);
    private static native String nativeGetVersion();

    @ReactMethod
    public void render(final String source, final Promise promise) {
        if (source == null) {
            promise.reject("NULL_INPUT", "render: source is null");
            return;
        }
        renderQueue.execute(() -> {
            try {
                int[] status = new int[]{ ERR_RENDER };
                String svg = nativeRender(source, status);
                if (svg == null || status[0] != OK) {
                    String code;
                    switch (status[0]) {
                        case ERR_PARSE:      code = "PARSE_ERROR"; break;
                        case ERR_RENDER:     code = "RENDER_ERROR"; break;
                        case ERR_NULL_INPUT: code = "NULL_INPUT"; break;
                        default:             code = "UNKNOWN"; break;
                    }
                    promise.reject(code, "supramark_mermaid_render returned " + status[0]);
                    return;
                }
                promise.resolve(svg);
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
                promise.reject("UNKNOWN", "supramark_mermaid_version returned NULL");
            } else {
                promise.resolve(v);
            }
        } catch (Throwable t) {
            promise.reject("UNKNOWN", t.toString(), t);
        }
    }
}
