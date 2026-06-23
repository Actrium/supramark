require "json"

package = JSON.parse(File.read(File.join(__dir__, "package.json")))

# Podspec is at the package root (not ios/) so Expo / RN autolinking discovers
# it via `findPodspecFile(packageRoot)`. All path references inside point under
# ios/ where the actual sources + vendored xcframework live.

Pod::Spec.new do |s|
  s.name         = "SupramarkMarkdownNative"
  s.version      = package["version"]
  s.summary      = package["description"]
  s.homepage     = package["repository"]["url"]
  s.license      = package["license"]
  s.authors      = package["authors"]
  s.source       = { :git => package["repository"]["url"], :tag => "v#{s.version}" }

  # iOS 15.1 matches what scripts/build-ios.sh / Xcode crosscompile target
  # the supramark-markdown-native staticlib for. Lowering this without a
  # rebuild would cause an SDK/ABI mismatch at link time.
  s.ios.deployment_target = "15.1"
  s.osx.deployment_target = "11.0"

  s.source_files = "ios/*.{h,m,mm}"
  s.public_header_files = "ios/SupramarkMarkdownModule.h"

  # The vendored xcframework is staged under ios/Frameworks/ by
  # scripts/prepare-native.js (consuming target/ios-xcframeworks/ from
  # the workspace root after cargo build + scripts/build-ios-xcframework.sh).
  s.preserve_paths = "ios/Frameworks/**"
  s.vendored_frameworks = "ios/Frameworks/SupramarkMarkdown.xcframework"
  s.xcconfig = {
    "HEADER_SEARCH_PATHS" =>
      "\"$(PODS_TARGET_SRCROOT)/ios/Frameworks/SupramarkMarkdown.xcframework/ios-arm64/Headers\" " \
      "\"$(PODS_TARGET_SRCROOT)/ios/Frameworks/SupramarkMarkdown.xcframework/ios-arm64_x86_64-simulator/Headers\"",
    "OTHER_LDFLAGS" => "$(inherited)",
  }

  if respond_to?(:install_modules_dependencies, true)
    install_modules_dependencies(s)
  else
    s.dependency "React-Core"
  end
end
