/*
 * ReactPackageProvider.h (Windows)
 *
 * Registers the SupramarkPlantumlModule with the React Native Windows runtime.
 *
 * SPDX-License-Identifier: GPL-3.0-or-later OR LGPL-3.0-or-later OR Apache-2.0 OR EPL-2.0 OR MIT
 */

#pragma once

#include <winrt/Microsoft.ReactNative.h>

namespace winrt::SupramarkPlantumlNative::implementation {

struct ReactPackageProvider
    : winrt::implements<ReactPackageProvider, winrt::Microsoft::ReactNative::IReactPackageProvider> {

    void CreatePackage(winrt::Microsoft::ReactNative::IReactPackageBuilder const& packageBuilder) noexcept;
};

} // namespace winrt::SupramarkPlantumlNative::implementation
