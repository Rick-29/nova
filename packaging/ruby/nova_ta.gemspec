# Gemspec for the UniFFI Ruby bindings of nova-ta.
# `cargo xtask build -t ruby` copies it next to lib/nova_ta.rb and the native library.
# The version is kept in sync with Cargo.toml by `cargo xtask check` / `cargo xtask bump`.

Gem::Specification.new do |spec|
  spec.name = "nova_ta"
  spec.version = "0.1.1"
  spec.summary = "Stateful technical-analysis indicators for real-time data."
  spec.description = "Ruby bindings (via UniFFI) for the nova-ta Rust crate."
  spec.authors = ["Rick-29"]
  spec.license = "MIT"
  spec.homepage = "https://github.com/Rick-29/nova"
  spec.required_ruby_version = ">= 3.0"

  spec.files = Dir["lib/**/*"]
  spec.require_paths = ["lib"]

  spec.add_dependency "ffi", "~> 1.16"
end
