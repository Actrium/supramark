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

  # Link against the prebuilt Graphviz wrapper downloaded by postinstall.
  s.preserve_paths = "Frameworks/**"
  s.xcconfig = {
    "HEADER_SEARCH_PATHS" => "\"$(PODS_TARGET_SRCROOT)/Frameworks/include\"",
    "LIBRARY_SEARCH_PATHS" => "\"$(PODS_TARGET_SRCROOT)/Frameworks/lib\"",
  }
  s.libraries = "graphviz_api"

  if respond_to?(:install_modules_dependencies, true)
    install_modules_dependencies(s)
  else
    s.dependency "React-Core"
  end
end
