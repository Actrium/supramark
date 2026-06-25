require "json"

package = JSON.parse(File.read(File.join(__dir__, "..", "package.json")))

Pod::Spec.new do |s|
  s.name         = "GraphvizNative"
  s.version      = package["version"]
  s.summary      = package["description"]
  s.homepage     = package["repository"]["url"]
  s.license      = package["license"]
  s.authors      = package["author"]
  s.source       = { :git => package["repository"]["url"], :tag => "v#{s.version}" }

  # iOS 15.1 matches the IOS_MIN_VERSION used in scripts/build-ios.sh to compile
  # the prebuilt staticlib. Lowering this value without rebuilding the prebuilt
  # would produce an ABI/SDK mismatch at link time.
  s.ios.deployment_target = "15.1"
  s.osx.deployment_target = "11.0"

  s.source_files = "*.{h,m,mm}"
  s.public_header_files = "GraphvizModule.h"

  # Link against the prebuilt Graphviz core, vendored as an XCFramework. It is
  # staged under ios/Frameworks/ by scripts/prepare-native.js (consuming
  # output/ios/Graphviz.xcframework produced by scripts/build-ios.sh) and ships
  # inside the npm tarball — there is no postinstall download.
  s.preserve_paths = "Frameworks/**"
  s.vendored_frameworks = "Frameworks/Graphviz.xcframework"
  s.xcconfig = {
    "HEADER_SEARCH_PATHS" =>
      "\"$(PODS_TARGET_SRCROOT)/Frameworks/Graphviz.xcframework/ios-arm64/Headers\" " \
      "\"$(PODS_TARGET_SRCROOT)/Frameworks/Graphviz.xcframework/ios-arm64_x86_64-simulator/Headers\"",
    "OTHER_LDFLAGS" => "$(inherited)",
  }

  if respond_to?(:install_modules_dependencies, true)
    install_modules_dependencies(s)
  else
    s.dependency "React-Core"
  end
end
