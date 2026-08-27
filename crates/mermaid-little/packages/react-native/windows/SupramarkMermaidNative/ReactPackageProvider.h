/*
 * ReactPackageProvider.h (Windows)
 *
 * Registers the SupramarkMermaidModule with the React Native Windows runtime.
 *
 * SPDX-License-Identifier: MPL-2.0
 */

#pragma once

#include <winrt/Microsoft.ReactNative.h>

namespace winrt::SupramarkMermaidNative::implementation {

struct ReactPackageProvider
    : winrt::implements<ReactPackageProvider, winrt::Microsoft::ReactNative::IReactPackageProvider> {

    void CreatePackage(winrt::Microsoft::ReactNative::IReactPackageBuilder const& packageBuilder) noexcept;
};

} // namespace winrt::SupramarkMermaidNative::implementation
