require "json"

package = JSON.parse(File.read(File.join(__dir__, "package.json")))

# Podspec is at the package root (not ios/) so Expo / RN autolinking discovers
# it via `findPodspecFile(packageRoot)`. All path references inside point under
# ios/ where the actual sources + vendored xcframework live.

Pod::Spec.new do |s|
  s.name         = "SupramarkD2Native"
  s.version      = package["version"]
  s.summary      = package["description"]
  s.homepage     = package["repository"]["url"]
  s.license      = package["license"]
  s.authors      = package["author"]
  s.source       = { :git => package["repository"]["url"], :tag => "v#{s.version}" }

  # iOS 15.1 matches what scripts/build-ios.sh / Xcode crosscompile target
  # the supramark-d2-native staticlib for. Lowering this without a rebuild
  # would cause an SDK/ABI mismatch at link time.
  s.ios.deployment_target = "15.1"
  s.osx.deployment_target = "11.0"

  s.source_files = "ios/*.{h,m,mm}"
  s.public_header_files = "ios/SupramarkD2Module.h"

  # iOS keeps the static XCFramework; macOS uses a dylib so multiple Rust
  # static libraries do not link duplicate runtimes into the host.
  s.preserve_paths = ["ios/Frameworks/**", "macos/Frameworks/**"]
  s.ios.vendored_frameworks = "ios/Frameworks/SupramarkD2.xcframework"
  s.osx.vendored_libraries = "macos/Frameworks/lib/libsupramark_d2_native.dylib"
  s.ios.xcconfig = {
    "HEADER_SEARCH_PATHS" =>
      "\"$(PODS_TARGET_SRCROOT)/ios/Frameworks/SupramarkD2.xcframework/ios-arm64/Headers\" " \
      "\"$(PODS_TARGET_SRCROOT)/ios/Frameworks/SupramarkD2.xcframework/ios-arm64_x86_64-simulator/Headers\"",
    "OTHER_LDFLAGS" => "$(inherited)",
  }
  s.osx.xcconfig = {
    "HEADER_SEARCH_PATHS" => "\"$(PODS_TARGET_SRCROOT)/macos/Frameworks/include\"",
    "OTHER_LDFLAGS" => "$(inherited)",
  }

  if respond_to?(:install_modules_dependencies, true)
    install_modules_dependencies(s)
  else
    s.dependency "React-Core"
  end
end
