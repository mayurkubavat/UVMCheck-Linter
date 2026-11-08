#include "uvmcheck/compat/Overlay.h"

#include "uvmcheck/core/Diagnostic.h"
#include "uvmcheck/core/ProjectConfig.h"

#include <algorithm>
#include <filesystem>
#include <fstream>
#include <iterator>
#include <set>
#include <string>
#include <vector>

#include "slang/driver/Driver.h"

namespace fs = std::filesystem;

namespace uvmcheck::compat {

namespace {

std::string canonical(const std::string& p) {
    std::error_code ec;
    auto c = fs::weakly_canonical(fs::absolute(p, ec), ec);
    return (ec ? fs::path(p) : c).generic_string();
}

/// The first line of an encryption envelope in `file`, or 0. Covers the IEEE
/// form (`pragma protect begin_protected) and the older `protected blocks.
size_t envelopeLine(const fs::path& file) {
    std::ifstream in(file, std::ios::binary);
    std::string line;
    for (size_t n = 1; std::getline(in, line); ++n) {
        if (line.find("begin_protected") != std::string::npos && line.find("pragma") != std::string::npos) return n;
        auto pos = line.find("`protected");
        if (pos != std::string::npos && line.find_first_not_of(" \t") == pos) return n;
    }
    return 0;
}

/// The name in a slang "unknown <kind> 'name'" error, or "" for any other message.
std::string unknownName(const std::string& message) {
    if (!message.starts_with("unknown ")) return {};
    auto open = message.find('\'');
    if (open == std::string::npos) return {};
    auto close = message.find('\'', open + 1);
    if (close == std::string::npos) return {};
    return message.substr(open + 1, close - open - 1);
}

constexpr std::string_view kStubExtensions[] = {".sv", ".v", ".svp", ".vp"};

/// Encrypted IP, as vendors ship VIP: slang cannot decrypt an envelope, so it
/// skips it, and every package, module or interface the envelope defines is
/// "unknown" to the code that uses it. Those errors say nothing about the user's
/// code. With this override, when the sources hold an envelope:
///  - "unknown package/module/interface/class 'X'" errors and the envelope's own
///    warnings collapse into one Info finding that names the missing names;
///  - compat.ip_stubs files (stub declarations of the IP's public API, as the
///    vendor documents it) are compiled as library code, so code that uses the IP
///    is checked against real types and only names the stubs lack stay unknown.
class OpaqueIp : public Override {
public:
    std::string_view id() const override { return "opaque-ip"; }
    std::string_view summary() const override {
        return "Encrypted IP: compile compat.ip_stubs as library code and fold the errors for names defined "
               "inside protected envelopes into one Info finding";
    }

    bool configureDriver(DriverContext& ctx) override {
        envelopes.clear();
        for (const auto& p : ctx.driver.sourceLoader.getFilePaths()) {
            if (auto line = envelopeLine(p)) envelopes.emplace_back(canonical(fs::path(p).string()), line);
        }
        for (const auto& stub : ctx.project.compat().ipStubs) addStubs(ctx, stub);
        return true;
    }

    void reviewFindings(slang::ast::Compilation&, std::vector<core::Violation>& findings) override {
        if (envelopes.empty()) return;
        std::set<std::string> envelopeFiles;
        for (const auto& e : envelopes) envelopeFiles.insert(e.first);

        std::vector<std::string> names;
        std::optional<core::Violation> first;
        std::erase_if(findings, [&](const core::Violation& v) {
            if (v.ruleId == "SV-CMP-002" && envelopeFiles.count(canonical(v.filePath))) return true;
            if (v.ruleId != "SV-CMP-001") return false;
            auto name = unknownName(v.message);
            if (name.empty()) return false;
            if (std::find(names.begin(), names.end(), name) == names.end()) names.push_back(name);
            if (!first) first = v;
            return true;
        });
        if (!first) return;

        std::string list;
        for (size_t i = 0; i < names.size() && i < 8; ++i) list += (i ? ", " : "") + names[i];
        if (names.size() > 8) list += " and " + std::to_string(names.size() - 8) + " more";
        core::Violation v = *first;
        v.ruleId = "SV-CMP-002";
        v.ruleName = "CompilerWarning";
        v.severity = core::Severity::Info;
        v.endColumn = 0;
        v.fixIts.clear();
        v.related.clear();
        v.message = std::to_string(names.size()) + " name(s) are unknown and the sources hold encrypted IP (" +
                    fs::path(envelopes.front().first).filename().string() + ":" +
                    std::to_string(envelopes.front().second) +
                    "), so they are taken to be defined inside it: " + list +
                    ". Code that uses them is linted without their types; list stub declarations under "
                    "compat.ip_stubs to check it. [opaque-ip]";
        findings.push_back(std::move(v));
    }

private:
    void addStubs(DriverContext& ctx, const std::string& stub) {
        std::error_code ec;
        fs::path path(stub);
        if (fs::is_directory(path, ec)) {
            ctx.driver.sourceManager.addUserDirectories(path.string());
            std::vector<fs::path> files;
            for (const auto& entry : fs::directory_iterator(path, ec)) {
                auto ext = entry.path().extension().string();
                if (std::find(std::begin(kStubExtensions), std::end(kStubExtensions), ext) != std::end(kStubExtensions))
                    files.push_back(entry.path());
            }
            std::sort(files.begin(), files.end());
            for (const auto& f : files) ctx.driver.sourceLoader.addFiles(f.string());
            ctx.project.libraryPaths().add(core::PathPattern((path / "**").generic_string(), "."));
        } else if (fs::is_regular_file(path, ec)) {
            ctx.driver.sourceLoader.addFiles(path.string());
            ctx.project.libraryPaths().add(core::PathPattern(path.generic_string(), "."));
        } else {
            ctx.log << "[uvmcheck] warning: opaque-ip: stub path '" << stub << "' does not exist\n";
            return;
        }
        ctx.log << "[uvmcheck] opaque-ip: compiling stubs from " << stub << "\n";
    }

    std::vector<std::pair<std::string, size_t>> envelopes;
};

/// Jasper's -automatic_global_clock: formal flows often use $global_clock without
/// a global clocking block, and Jasper supplies one. Slang (issue #2043) reports
/// "no global clocking is available" for each use. With this override those
/// errors fold into one Info finding, so assertion code written for Jasper lints
/// clean; the rest of the assertion is still checked.
class GlobalClock : public Override {
public:
    std::string_view id() const override { return "global-clock"; }
    std::string_view summary() const override {
        return "Accept $global_clock without a global clocking block, as Jasper's -automatic_global_clock does";
    }
    std::string_view upstream() const override { return "https://github.com/MikePopoloski/slang/issues/2043"; }

    void reviewFindings(slang::ast::Compilation&, std::vector<core::Violation>& findings) override {
        size_t count = 0;
        std::optional<core::Violation> first;
        std::erase_if(findings, [&](const core::Violation& v) {
            if (v.ruleId != "SV-CMP-001" || v.message.find("no global clocking is available") == std::string::npos)
                return false;
            if (!first) first = v;
            ++count;
            return true;
        });
        if (!first) return;
        core::Violation v = *first;
        v.ruleId = "SV-CMP-002";
        v.ruleName = "CompilerWarning";
        v.severity = core::Severity::Info;
        v.message = std::to_string(count) +
                    " use(s) of $global_clock with no global clocking block in scope; accepted as Jasper's "
                    "-automatic_global_clock would. Declare 'global clocking @(posedge clk); endclocking' for "
                    "simulators. [global-clock]";
        findings.push_back(std::move(v));
    }
};

} // namespace

std::unique_ptr<Override> createOpaqueIp() {
    return std::make_unique<OpaqueIp>();
}

std::unique_ptr<Override> createGlobalClock() {
    return std::make_unique<GlobalClock>();
}

} // namespace uvmcheck::compat
