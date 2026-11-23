require "json"

package = JSON.parse(File.read(File.join(__dir__, "../../package.json")))
Pod::Spec.new do |s|
  s.name = "SupramarkSelectionMetrics"
  s.version = package["version"]
  s.summary = "TextKit caret positions for Supramark's macOS document selection"
  s.homepage = "https://github.com/Actrium/supramark"
  s.source = { :git => "#{s.homepage}.git", :tag => "v#{s.version}" }
  s.license = { :type => "Apache-2.0" }
  s.author = "Supramark contributors"
  s.platform = :osx, "11.0"
  s.source_files = "*.{h,mm}"
  install_modules_dependencies(s)
end
