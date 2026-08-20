/*
 * ReactPackageProvider.h (Windows)
 *
 * Registers the SupramarkD2Module with the React Native Windows runtime.
 *
 * SPDX-License-Identifier: MPL-2.0
 */

#pragma once

#include <winrt/Microsoft.ReactNative.h>

namespace winrt::SupramarkD2Native::implementation {

struct ReactPackageProvider
    : winrt::implements<ReactPackageProvider, winrt::Microsoft::ReactNative::IReactPackageProvider> {

    void CreatePackage(winrt::Microsoft::ReactNative::IReactPackageBuilder const& packageBuilder) noexcept;
};

} // namespace winrt::SupramarkD2Native::implementation
