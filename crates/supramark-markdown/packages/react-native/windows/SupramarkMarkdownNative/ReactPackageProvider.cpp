/*
 * ReactPackageProvider.cpp (Windows)
 *
 * Registers the SupramarkMarkdownModule with the React Native Windows runtime.
 *
 * Licensed under the Apache License, Version 2.0
 */

#include "ReactPackageProvider.h"
#include "SupramarkMarkdownModule.h"

#include <NativeModules.h>

namespace winrt::SupramarkMarkdownNative::implementation {

void ReactPackageProvider::CreatePackage(
    winrt::Microsoft::ReactNative::IReactPackageBuilder const& packageBuilder) noexcept {
    AddAttributedModules(packageBuilder, true);
}

} // namespace winrt::SupramarkMarkdownNative::implementation
