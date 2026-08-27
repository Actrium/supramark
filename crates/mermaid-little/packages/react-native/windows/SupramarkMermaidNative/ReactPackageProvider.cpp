/*
 * ReactPackageProvider.cpp (Windows)
 *
 * Registers the SupramarkMermaidModule with the React Native Windows runtime.
 *
 * SPDX-License-Identifier: MPL-2.0
 */

#include "ReactPackageProvider.h"
#include "SupramarkMermaidModule.h"

#include <NativeModules.h>

namespace winrt::SupramarkMermaidNative::implementation {

void ReactPackageProvider::CreatePackage(
    winrt::Microsoft::ReactNative::IReactPackageBuilder const& packageBuilder) noexcept {
    AddAttributedModules(packageBuilder, true);
}

} // namespace winrt::SupramarkMermaidNative::implementation
