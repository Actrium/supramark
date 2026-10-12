/*
 * ReactPackageProvider.cpp (Windows)
 *
 * Registers the SupramarkD2Module with the React Native Windows runtime.
 *
 * SPDX-License-Identifier: MPL-2.0
 */

#include "ReactPackageProvider.h"
#include "SupramarkD2Module.h"

#include <NativeModules.h>

namespace winrt::SupramarkD2Native::implementation {

void ReactPackageProvider::CreatePackage(
    winrt::Microsoft::ReactNative::IReactPackageBuilder const& packageBuilder) noexcept {
    AddAttributedModules(packageBuilder, true);
}

} // namespace winrt::SupramarkD2Native::implementation
