#include "uvmcheck/compat/Overlay.h"

#include "uvmcheck/core/ProjectConfig.h"

#include <algorithm>
#include <filesystem>
#include <fstream>
#include <iterator>

#include "slang/driver/Driver.h"
#include "slang/util/VersionInfo.h"

namespace fs = std::filesystem;

namespace uvmcheck::compat {

std::unique_ptr<Override> createVendorSystemTasks();
std::unique_ptr<Override> createErrorTaint();
std::unique_ptr<Override> createErrorAnchor();
std::unique_ptr<Override> createCrashGuard();
std::unique_ptr<Override> createOpaqueIp();
std::unique_ptr<Override> createGlobalClock();

std::string VendorArgs::firstSeen() const {
    if (!includeDirs.empty()) return "-i";
    if (uvm.has_value()) return "-uvm";
    if (uvmHome.has_value()) return "-uvmhome";
    if (!ntbOpts.empty()) return "-ntb_opts";
    return {};
}

bool VendorArgs::requestsUvm(const std::vector<std::string>& libraryOrder) const {
    if (uvm.value_or(false) || uvmHome.has_value()) return true;
    auto isUvm = [](const std::string& s) { return s.rfind("uvm", 0) == 0; };
    return std::any_of(ntbOpts.begin(), ntbOpts.end(), isUvm) ||
           std::any_of(libraryOrder.begin(), libraryOrder.end(), isUvm);
}

namespace {

bool isUvmPackageFile(const fs::path& p) {
    return p.filename() == "uvm_pkg.sv";
}

/// The directory holding uvm_pkg.sv under a uvm-core checkout (or its src/).
std::optional<fs::path> uvmSourceDir(const std::string& home) {
    if (home.empty()) return std::nullopt;
    std::error_code ec;
    for (fs::path dir : {fs::path(home) / "src", fs::path(home)}) {
        if (fs::is_regular_file(dir / "uvm_pkg.sv", ec)) return dir.lexically_normal();
    }
    return std::nullopt;
}

/// True when one of the named files mentions the UVM package or its macros.
bool sourcesUseUvm(const std::vector<fs::path>& files) {
    for (const auto& f : files) {
        std::ifstream in(f, std::ios::binary);
        if (!in) continue;
        std::string text((std::istreambuf_iterator<char>(in)), std::istreambuf_iterator<char>());
        if (text.find("uvm_pkg") != std::string::npos || text.find("uvm_macros.svh") != std::string::npos)
            return true;
    }
    return false;
}

/// Simulator options from file lists: -i <dir> is an include directory; the
/// UVM requests (-uvm, -uvmhome, -ntb_opts uvm, -L uvm) are acted on by auto-uvm.
class FilelistDialect : public Override {
public:
    std::string_view id() const override { return "filelist-dialect"; }
    std::string_view summary() const override {
        return "Accept simulator file-list options slang does not know: -i <dir>, -uvm, -uvmhome <dir>, "
               "-ntb_opts <opt>";
    }

    bool configureDriver(DriverContext& ctx) override {
        for (const auto& dir : ctx.vendor.includeDirs) {
            if (auto ec = ctx.driver.sourceManager.addUserDirectories(dir))
                ctx.log << "[uvmcheck] warning: include dir '" << dir << "': " << ec.message() << "\n";
        }
        return true;
    }
};

/// Adds the UVM library when the code uses it but no uvm_pkg.sv was given, as
/// simulators that ship UVM (-L uvm, -uvm, -ntb_opts uvm) expect.
class AutoUvm : public Override {
public:
    std::string_view id() const override { return "auto-uvm"; }
    std::string_view summary() const override {
        return "Compile uvm_pkg.sv from compat.uvm_home, -uvmhome or $UVM_HOME when the sources use UVM "
               "but do not list it";
    }

    bool configureDriver(DriverContext& ctx) override {
        std::vector<fs::path> files;
        for (const auto& p : ctx.driver.sourceLoader.getFilePaths()) files.emplace_back(p);
        if (std::any_of(files.begin(), files.end(), isUvmPackageFile)) return true;
        if (!ctx.vendor.requestsUvm(ctx.driver.options.libraryOrder) && !sourcesUseUvm(files)) return true;

        auto dir = uvmSourceDir(ctx.uvmHome);
        if (!dir) {
            ctx.log << "[uvmcheck] warning: auto-uvm: the sources use UVM but no uvm_pkg.sv was found"
                    << (ctx.uvmHome.empty() ? std::string() : " under '" + ctx.uvmHome + "'")
                    << "; set compat.uvm_home, -uvmhome or $UVM_HOME\n";
            return true;
        }
        if (auto ec = ctx.driver.sourceManager.addUserDirectories(dir->string()))
            ctx.log << "[uvmcheck] warning: auto-uvm: include dir '" << dir->string() << "': " << ec.message()
                    << "\n";
        ctx.driver.sourceLoader.addFiles((*dir / "uvm_pkg.sv").string());
        ctx.log << "[uvmcheck] auto-uvm: compiling " << (*dir / "uvm_pkg.sv").string() << "\n";
        return true;
    }
};

/// Treats the directory of every uvm_pkg.sv being compiled as library code, so
/// warnings and findings inside the UVM library are not reported.
class UvmLibrary : public Override {
public:
    std::string_view id() const override { return "uvm-library"; }
    std::string_view summary() const override {
        return "Treat the UVM library being compiled (the folder of uvm_pkg.sv) as library code: compiled, never "
               "reported";
    }

    bool configureDriver(DriverContext& ctx) override {
        for (const auto& p : ctx.driver.sourceLoader.getFilePaths()) {
            fs::path file(p);
            if (!isUvmPackageFile(file)) continue;
            std::error_code ec;
            fs::path dir = fs::weakly_canonical(fs::absolute(file, ec), ec).parent_path();
            if (dir.empty()) continue;
            // uvm-core keeps the package in src/; its macros and base classes sit beside it.
            ctx.project.libraryPaths().add(core::PathPattern((dir / "**").generic_string(), "."));
        }
        return true;
    }
};

/// Slang's own VCS compatibility mode (--compat vcs), for the vcs profile.
class SlangCompatVcs : public Override {
public:
    std::string_view id() const override { return "slang-compat-vcs"; }
    std::string_view summary() const override {
        return "Turn on slang's --compat vcs (relaxed conversions and VCS-style include search)";
    }

    bool configureDriver(DriverContext& ctx) override {
        if (!ctx.driver.options.compat.has_value()) ctx.driver.options.compat = slang::driver::CompatMode::Vcs;
        return true;
    }
};

/// Selects IEEE 1800-2023 unless --std chose a version. Slang defaults to
/// 1800-2017, where 2023 constructs (final classes, :initial/:extends/:final
/// specifiers, triple-quoted strings, real constraints, covergroup extends, ...)
/// are errors. 2023 adds no keywords, so 2017 code compiles the same.
class LangStd : public Override {
public:
    std::string_view id() const override { return "lang-std"; }
    std::string_view summary() const override {
        return "Compile as IEEE 1800-2023 unless --std names another version (slang's default is 1800-2017)";
    }
    bool onByDefault() const override { return true; }

    bool configureDriver(DriverContext& ctx) override {
        if (!ctx.driver.options.languageVersion.has_value()) ctx.driver.options.languageVersion = "1800-2023";
        return true;
    }
};

std::vector<std::unique_ptr<Override>> makeAll() {
    std::vector<std::unique_ptr<Override>> v;
    v.push_back(std::make_unique<LangStd>());
    v.push_back(createCrashGuard());
    v.push_back(std::make_unique<FilelistDialect>());
    // auto-uvm adds uvm_pkg.sv before uvm-library looks for it.
    v.push_back(std::make_unique<AutoUvm>());
    v.push_back(std::make_unique<UvmLibrary>());
    // Before error-taint, so the errors it folds do not taint findings.
    v.push_back(createOpaqueIp());
    v.push_back(createGlobalClock());
    v.push_back(createVendorSystemTasks());
    v.push_back(std::make_unique<SlangCompatVcs>());
    v.push_back(createErrorTaint());
    v.push_back(createErrorAnchor());
    return v;
}

struct Profile {
    std::string_view name;
    std::vector<std::string_view> ids;
};

const std::vector<Profile>& profiles() {
    static const std::vector<Profile> p = {
        {"none", {}},
        {"questa", {"filelist-dialect", "auto-uvm", "uvm-library", "vendor-systasks", "anchor-to-user-code"}},
        {"xcelium", {"filelist-dialect", "auto-uvm", "uvm-library", "vendor-systasks", "anchor-to-user-code"}},
        {"vcs",
         {"filelist-dialect", "auto-uvm", "uvm-library", "vendor-systasks", "anchor-to-user-code", "slang-compat-vcs"}},
        {"jasper", {"filelist-dialect", "global-clock", "vendor-systasks", "anchor-to-user-code"}},
    };
    return p;
}

} // namespace

const std::vector<std::unique_ptr<Override>>& Overlay::all() {
    static const auto overrides = makeAll();
    return overrides;
}

std::vector<std::string_view> Overlay::profile(std::string_view name) {
    for (const auto& p : profiles()) {
        if (p.name == name) return p.ids;
    }
    return {};
}

std::vector<std::string_view> Overlay::profileNames() {
    std::vector<std::string_view> names;
    for (const auto& p : profiles()) names.push_back(p.name);
    return names;
}

bool Overlay::isRetired(const Override& o) {
    return o.retireAtMajor() != 0 && slang::VersionInfo::getMajor() >= o.retireAtMajor();
}

bool Overlay::select(const core::CompatSettings& settings, std::string& error) {
    auto find = [](std::string_view id) -> Override* {
        for (const auto& o : all()) {
            if (o->id() == id) return o.get();
        }
        return nullptr;
    };

    std::vector<Override*> picked;
    auto add = [&](Override* o) {
        if (std::find(picked.begin(), picked.end(), o) == picked.end()) picked.push_back(o);
    };
    for (const auto& o : all()) {
        if (o->onByDefault()) add(o.get());
    }
    if (!settings.profile.empty()) {
        const auto names = profileNames();
        if (std::find(names.begin(), names.end(), settings.profile) == names.end()) {
            error = "unknown compat profile '" + settings.profile + "'";
            return false;
        }
        for (auto id : profile(settings.profile)) add(find(id));
    }
    for (const auto& entry : settings.overrides) {
        std::string_view id = entry;
        const bool remove = id.starts_with('-');
        if (remove || id.starts_with('+')) id.remove_prefix(1);
        Override* o = find(id);
        if (!o) {
            error = "unknown override '" + std::string(id) + "' (see --list-overrides)";
            return false;
        }
        if (remove) std::erase(picked, o);
        else add(o);
    }

    // Keep registration order, which is the order the hooks must run in, and
    // leave out overrides the linked slang no longer needs.
    chosen.clear();
    for (const auto& o : all()) {
        if (std::find(picked.begin(), picked.end(), o.get()) != picked.end() && !isRetired(*o))
            chosen.push_back(o.get());
    }
    return true;
}

bool Overlay::isActive(std::string_view id) const {
    return std::any_of(chosen.begin(), chosen.end(), [&](const Override* o) { return o->id() == id; });
}

std::vector<std::string> Overlay::activeIds() const {
    std::vector<std::string> ids;
    for (const auto* o : chosen) ids.emplace_back(o->id());
    return ids;
}

bool Overlay::configureDriver(DriverContext& ctx) const {
    for (auto* o : chosen) {
        if (!o->configureDriver(ctx)) return false;
    }
    return true;
}

void Overlay::configureCompilation(slang::ast::Compilation& compilation) const {
    for (auto* o : chosen) o->configureCompilation(compilation);
}

void Overlay::reviewFindings(slang::ast::Compilation& compilation, std::vector<core::Violation>& findings) const {
    for (auto* o : chosen) o->reviewFindings(compilation, findings);
}

void Overlay::list(std::ostream& os) {
    os << "Slang overlay overrides (off unless a compat profile or --override turns them on; those marked\n"
          "[on by default] apply unless --override -<id> turns them off):\n";
    for (const auto& o : all()) {
        os << "  " << o->id();
        if (o->onByDefault()) os << "  [on by default]";
        if (isRetired(*o)) os << "  [retired: slang " << slang::VersionInfo::getMajor() << " has the fix]";
        os << "\n      " << o->summary() << "\n";
        std::string in;
        for (const auto& p : profiles()) {
            const auto& ids = p.ids;
            if (std::find(ids.begin(), ids.end(), o->id()) != ids.end()) in += (in.empty() ? "" : ", ") + std::string(p.name);
        }
        if (!in.empty()) os << "      profiles: " << in << "\n";
        if (!o->upstream().empty()) os << "      upstream: " << o->upstream() << "\n";
    }
}

} // namespace uvmcheck::compat
