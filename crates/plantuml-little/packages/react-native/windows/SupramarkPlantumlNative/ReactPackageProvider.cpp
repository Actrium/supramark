/*
 * ReactPackageProvider.cpp (Windows)
 *
 * Registers the SupramarkPlantumlModule with the React Native Windows runtime.
 *
 * SPDX-License-Identifier: GPL-3.0-or-later OR LGPL-3.0-or-later OR Apache-2.0 OR EPL-2.0 OR MIT
 */

#include "ReactPackageProvider.h"
#include "SupramarkPlantumlModule.h"

#include <NativeModules.h>

namespace winrt::SupramarkPlantumlNative::implementation {

void ReactPackageProvider::CreatePackage(
    winrt::Microsoft::ReactNative::IReactPackageBuilder const& packageBuilder) noexcept {
    AddAttributedModules(packageBuilder, true);
}

} // namespace winrt::SupramarkPlantumlNative::implementation
