/*
 * ReactPackageProvider.h (Windows)
 *
 * Registers the SupramarkPlantumlModule with the React Native Windows runtime.
 * Declared as a WinRT runtimeclass (see ReactPackageProvider.idl) so RNW
 * autolinking can discover and instantiate it from the app's generated
 * AutolinkedNativeModules.g.cpp.
 *
 * SPDX-License-Identifier: GPL-3.0-or-later OR LGPL-3.0-or-later OR Apache-2.0 OR EPL-2.0 OR MIT
 */

#pragma once

#include "ReactPackageProvider.g.h"

using namespace winrt::Microsoft::ReactNative;

namespace winrt::SupramarkPlantumlNative::implementation
{
struct ReactPackageProvider : ReactPackageProviderT<ReactPackageProvider>
{
  ReactPackageProvider() = default;

  void CreatePackage(IReactPackageBuilder const &packageBuilder) noexcept;
};

} // namespace winrt::SupramarkPlantumlNative::implementation

namespace winrt::SupramarkPlantumlNative::factory_implementation
{
struct ReactPackageProvider
    : ReactPackageProviderT<ReactPackageProvider, implementation::ReactPackageProvider>
{};
} // namespace winrt::SupramarkPlantumlNative::factory_implementation
