/*
 * SupramarkMarkdownPackage.java — RN package registration entrypoint.
 *
 * Hosts add `new SupramarkMarkdownPackage()` to their list of packages in
 * MainApplication.java (old arch). Under the new architecture this is
 * picked up via codegen + autolinking and the manual registration is
 * not needed.
 */

package com.supramark.markdownnative;

import androidx.annotation.NonNull;

import com.facebook.react.ReactPackage;
import com.facebook.react.bridge.NativeModule;
import com.facebook.react.bridge.ReactApplicationContext;
import com.facebook.react.uimanager.ViewManager;

import java.util.ArrayList;
import java.util.Collections;
import java.util.List;

public class SupramarkMarkdownPackage implements ReactPackage {

    @Override
    @NonNull
    public List<NativeModule> createNativeModules(@NonNull ReactApplicationContext reactContext) {
        List<NativeModule> modules = new ArrayList<>();
        modules.add(new SupramarkMarkdownModule(reactContext));
        return modules;
    }

    @Override
    @NonNull
    public List<ViewManager> createViewManagers(@NonNull ReactApplicationContext reactContext) {
        return Collections.emptyList();
    }
}
