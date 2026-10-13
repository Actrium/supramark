/*
 * ReactPackageProvider.cpp (Windows)
 *
 * Registers the SupramarkPlantumlModule with the React Native Windows runtime.
 * Compiling SupramarkPlantumlModule.h into this translation unit is what makes
 * AddAttributedModules() discover the REACT_MODULE attributed module.
 *
 * SPDX-License-Identifier: GPL-3.0-or-later OR LGPL-3.0-or-later OR Apache-2.0 OR EPL-2.0 OR MIT
 */

#include "pch.h"

#include "ReactPackageProvider.h"
#if __has_include("ReactPackageProvider.g.cpp")
#include "ReactPackageProvider.g.cpp"
#endif

#include <ModuleRegistration.h>

#include "SupramarkPlantumlModule.h"

using namespace winrt::Microsoft::ReactNative;

namespace winrt::SupramarkPlantumlNative::implementation
{
void ReactPackageProvider::CreatePackage(IReactPackageBuilder const &packageBuilder) noexcept
{
  AddAttributedModules(packageBuilder, true);
}

} // namespace winrt::SupramarkPlantumlNative::implementation
