#pragma once

#include <memory>
#include <optional>
#include <ostream>
#include <string>
#include <string_view>
#include <vector>

namespace slang::driver {
class Driver;
}
namespace slang::ast {
class Compilation;
}

namespace uvmcheck::core {
class ProjectConfig;
struct CompatSettings;
} // namespace uvmcheck::core

namespace uvmcheck::compat {

/// Simulator options seen on the command line or in file lists. Slang does not
/// know them; the driver collects them so the overlay can act on them (or say
/// which override would).
struct VendorArgs {
    std::vector<std::string> includeDirs; // -i <dir>
    std::optional<bool> uvm;              // -uvm (Xcelium)
    std::optional<std::string> uvmHome;   // -uvmhome <dir> (Xcelium)
    std::vector<std::string> ntbOpts;     // -ntb_opts <opt> (VCS)

    /// The first vendor option given, as written, or "" when there were none.
    std::string firstSeen() const;
    /// True when an option asks the simulator to supply the UVM library.
    bool requestsUvm(const std::vector<std::string>& libraryOrder) const;
};

/// What an override may read and change while the driver is being set up.
struct DriverContext {
    slang::driver::Driver& driver;
    core::ProjectConfig& project;
    const VendorArgs& vendor;
    std::string uvmHome; // configured, -uvmhome, or $UVM_HOME; may be empty
    std::ostream& log;
};

/// One opt-in change to how the linter drives slang. Each override hooks into
/// one stage, uses only slang's public API, and names the slang version that
/// makes it unnecessary, so workarounds do not outlive the bug they cover.
class Override {
public:
    virtual ~Override() = default;

    virtual std::string_view id() const = 0;
    virtual std::string_view summary() const = 0;
    /// Upstream issue or pull request, when there is one.
    virtual std::string_view upstream() const { return {}; }
    /// The first slang major version that no longer needs this override; 0 = never.
    virtual int retireAtMajor() const { return 0; }

    /// After the command line and project config are read, before slang
    /// processes its options and loads sources. Returns false to stop the run.
    virtual bool configureDriver(DriverContext&) { return true; }
    /// After the compilation is created, before anything elaborates.
    virtual void configureCompilation(slang::ast::Compilation&) {}
};

/// The registry of overrides and the set chosen for this run.
class Overlay {
public:
    /// Every override the linter knows, in registration order.
    static const std::vector<std::unique_ptr<Override>>& all();
    /// Override ids a profile turns on; empty for an unknown profile.
    static std::vector<std::string_view> profile(std::string_view name);
    static std::vector<std::string_view> profileNames();
    /// True when the linked slang already has the fix the override stands in for.
    static bool isRetired(const Override& o);

    /// Chooses the active overrides: the profile's, then each "+id" / "id"
    /// added and each "-id" removed. Returns false with `error` set for an
    /// unknown profile or id.
    bool select(const core::CompatSettings& settings, std::string& error);

    const std::vector<Override*>& active() const { return chosen; }
    bool isActive(std::string_view id) const;
    /// Active override ids, for reports and the cache key.
    std::vector<std::string> activeIds() const;

    bool configureDriver(DriverContext& ctx) const;
    void configureCompilation(slang::ast::Compilation& compilation) const;

    /// Prints every override, the profiles that include it, and whether it is retired.
    static void list(std::ostream& os);

private:
    std::vector<Override*> chosen;
};

} // namespace uvmcheck::compat
