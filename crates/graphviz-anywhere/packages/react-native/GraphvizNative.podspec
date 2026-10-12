require "json"

package = JSON.parse(File.read(File.join(__dir__, "package.json")))

Pod::Spec.new do |s|
  s.name         = "GraphvizNative"
  s.version      = package["version"]
  s.summary      = package["description"]
  s.homepage     = package["repository"]["url"]
  s.license      = package["license"]
  s.authors      = package["author"]
  s.source       = { :git => package["repository"]["url"], :tag => "v#{s.version}" }

  # Keep Pod deployment targets aligned with the prebuilt Apple binaries.
  s.ios.deployment_target = "15.1"
  s.osx.deployment_target = "11.0"

  # Compile only the wrapper for the active Apple platform.
  s.ios.source_files = "ios/*.{h,m,mm}"
  s.ios.public_header_files = "ios/GraphvizModule.h"
  s.osx.source_files = "macos/*.{h,m,mm}"
  s.osx.public_header_files = "macos/GraphvizModule.h"

  # iOS keeps the static XCFramework; macOS uses a dylib to avoid colliding
  # with the Graphviz symbols embedded by PlantUML.
  s.preserve_paths = ["ios/Frameworks/**", "macos/Frameworks/**"]
  s.ios.vendored_frameworks = "ios/Frameworks/Graphviz.xcframework"
  s.osx.vendored_libraries = "macos/Frameworks/lib/libgraphviz_api.dylib"
  s.libraries = "c++", "expat", "z"
  s.ios.xcconfig = {
    "HEADER_SEARCH_PATHS" =>
      "\"$(PODS_TARGET_SRCROOT)/ios/Frameworks/Graphviz.xcframework/ios-arm64/Headers\" " \
      "\"$(PODS_TARGET_SRCROOT)/ios/Frameworks/Graphviz.xcframework/ios-arm64_x86_64-simulator/Headers\"",
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
