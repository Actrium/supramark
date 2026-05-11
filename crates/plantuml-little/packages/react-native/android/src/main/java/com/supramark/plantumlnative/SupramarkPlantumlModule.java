/*
 * SupramarkPlantumlModule.java — RN bridge module for d2 native FFI.
 *
 * Loads libsupramark_plantuml_jni.so (the JNI shim, which in turn links
 * libsupramark_plantuml_native.so), dispatches render calls off the JS
 * thread, and resolves promises with the produced SVG.
 */

package com.supramark.plantumlnative;

import androidx.annotation.NonNull;

import com.facebook.react.bridge.Promise;
import com.facebook.react.bridge.ReactApplicationContext;
import com.facebook.react.bridge.ReactContextBaseJavaModule;
import com.facebook.react.bridge.ReactMethod;

import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;

public class SupramarkPlantumlModule extends ReactContextBaseJavaModule {

    public static final String NAME = "SupramarkPlantumlNative";

    private static final int OK             = 0;
    private static final int ERR_PARSE      = 1;
    private static final int ERR_RENDER     = 2;
    private static final int ERR_NULL_INPUT = 3;

    private static final boolean NATIVE_AVAILABLE;
    static {
        boolean ok = false;
        // libgraphviz_api.so (linked in transitively by libsupramark_plantuml_native.so)
        // references std::* typeinfo from libc++_shared.so but its NEEDED entry
        // doesn't list it. Pre-load c++_shared so the symbols resolve at dlopen
        // time when CMake's IMPORTED chain pulls plantuml in. On Android linker
        // namespaces the pre-load alone is sometimes insufficient — guard the
        // whole chain so a missing dep doesn't crash the host app at class init.
        try {
            try { System.loadLibrary("c++_shared"); } catch (UnsatisfiedLinkError ignore) {}
            System.loadLibrary("supramark_plantuml_jni");
            ok = true;
        } catch (UnsatisfiedLinkError e) {
            android.util.Log.e("SupramarkPlantumlNative",
                "Failed to load libsupramark_plantuml_jni or its dependencies; " +
                "render() will reject. Cause: " + e.getMessage());
        }
        NATIVE_AVAILABLE = ok;
    }

    private final ExecutorService renderQueue =
        Executors.newSingleThreadExecutor(r -> {
            Thread t = new Thread(r, "supramark-plantuml-native-render");
            t.setDaemon(true);
            return t;
        });

    public SupramarkPlantumlModule(ReactApplicationContext reactContext) {
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
        if (!NATIVE_AVAILABLE) {
            promise.reject("NATIVE_UNAVAILABLE",
                "libsupramark_plantuml_native.so (or its libgraphviz_api.so dep) " +
                "failed to load; see logcat at startup for the underlying dlopen error.");
            return;
        }
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
                    promise.reject(code, "supramark_plantuml_render returned " + status[0]);
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
        if (!NATIVE_AVAILABLE) {
            promise.reject("NATIVE_UNAVAILABLE",
                "libsupramark_plantuml_native.so (or its libgraphviz_api.so dep) " +
                "failed to load; see logcat at startup for the underlying dlopen error.");
            return;
        }
        try {
            String v = nativeGetVersion();
            if (v == null) {
                promise.reject("UNKNOWN", "supramark_plantuml_version returned NULL");
            } else {
                promise.resolve(v);
            }
        } catch (Throwable t) {
            promise.reject("UNKNOWN", t.toString(), t);
        }
    }
}
