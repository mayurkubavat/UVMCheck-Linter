#pragma once

#include <map>
#include <memory>
#include <string>
#include <string_view>
#include <vector>

namespace slang::ast {
class Compilation;
}

namespace uvmcheck::uvm {

/// What a UVM release says about one class or member: `ieee` when IEEE 1800.2
/// defines it, `accellera` for a detail of the Accellera implementation that is
/// not part of the standard, `contrib` for an item contributed for a later revision.
struct ApiEntry {
    std::string tag;
    std::string standard; ///< e.g. "1800.2-2020"; empty when the tag names none
    std::string clause;   ///< e.g. "5.3.4.1"; empty when the tag names none
};

/// The API of one UVM release, generated from its source tags by
/// tools/codegen/gen_uvm_api_pack.py (data/uvm-api/<version>.json, compiled into the
/// binary). Supporting a new release is adding its pack; no rule changes.
///
/// UVMCHECK_UVM_API_PACK=<file.json> replaces the built-in packs with one file.
class ApiPack {
public:
    /// The pack for the UVM compiled into `compilation`: its own version when a
    /// pack exists for it, else the newest pack of the same line (2020, 2017, 1.x),
    /// else the newest pack. Null only when no pack is available at all.
    static const ApiPack* forCompilation(const slang::ast::Compilation& compilation);

    /// The UVM version compiled into `compilation` ("2020.3.1", "2020-2.0",
    /// "1.2"), from uvm_pkg's UVM_VERSION_STRING or uvm_revision; empty when
    /// neither is there (no UVM, or the test stub).
    static std::string detectVersion(const slang::ast::Compilation& compilation);

    /// Normalizes a version string as written in uvm_version.svh to a pack name.
    static std::string normalizeVersion(std::string_view versionString);

    /// Every built-in pack version, oldest first.
    static std::vector<std::string> builtinVersions();

    const std::string& version() const { return versionName; }
    const ApiEntry* findClass(std::string_view cls) const;
    const ApiEntry* findMember(std::string_view cls, std::string_view member) const;
    /// True when the release compiles this macro only as deprecated API (see
    /// deprecatedGuard()).
    bool isDeprecatedMacro(std::string_view name) const;
    /// The define deprecated API hangs on: UVM_ENABLE_DEPRECATED_API (2020: it
    /// must be set) or UVM_NO_DEPRECATED (1.x: setting it removes the API).
    const std::string& deprecatedGuard() const { return guard; }
    bool hasMacro(std::string_view name) const;

    /// Parses a pack; null with `error` set when the text is not one.
    static std::unique_ptr<ApiPack> parse(std::string_view json, std::string& error);

private:
    struct ClassInfo {
        ApiEntry entry; // tag empty when the class itself carries none
        std::map<std::string, ApiEntry, std::less<>> members;
    };
    std::string versionName;
    std::string guard = "UVM_ENABLE_DEPRECATED_API";
    std::map<std::string, ClassInfo, std::less<>> classes;
    std::map<std::string, bool, std::less<>> macros; // name -> deprecated
};

} // namespace uvmcheck::uvm
