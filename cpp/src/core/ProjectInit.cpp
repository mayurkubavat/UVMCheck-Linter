#include "uvmcheck/core/ProjectInit.h"

#include "uvmcheck/rules/SweSupport.h"
#include "uvmcheck/style/NamingEngine.h"
#include "uvmcheck/uvm/UvmModel.h"

#include "slang/driver/Driver.h"
#include "slang/text/SourceManager.h"

#include <algorithm>
#include <cstdlib>
#include <fstream>
#include <map>
#include <regex>
#include <set>
#include <sstream>

namespace uvmcheck::core::init {

namespace fs = std::filesystem;

namespace {

constexpr size_t kMaxFiles = 50000;

bool skipDir(const fs::path& dir) {
    std::string name = dir.filename().string();
    if (name.empty()) return false;
    if (name[0] == '.') return true; // .git, .uvmcheck, editor folders
    static const std::set<std::string> skip = {"build", "node_modules", "work", "sim_build", "xcelium.d",
                                               "csrc", "simv.daidir", "__pycache__", "INCA_libs", "obj_dir"};
    return skip.count(name) > 0 || name.starts_with("build-") || name.starts_with("cmake-build");
}

std::string readText(const fs::path& p) {
    std::ifstream in(p, std::ios::binary);
    return std::string((std::istreambuf_iterator<char>(in)), std::istreambuf_iterator<char>());
}

std::string rel(const fs::path& p, const fs::path& root) {
    std::error_code ec;
    fs::path r = fs::relative(p, root, ec);
    if (ec || r.empty() || *r.begin() == "..") return p.generic_string();
    return r.generic_string();
}

bool isUnder(const fs::path& p, const fs::path& dir) {
    if (dir.empty()) return false;
    auto pi = p.begin();
    for (auto di = dir.begin(); di != dir.end(); ++di, ++pi) {
        if (pi == p.end() || *pi != *di) return false;
    }
    return true;
}

/// True if slang can read the file list and it names at least one source.
bool usableFilelist(const fs::path& f) {
    slang::driver::Driver probe;
    probe.addStandardArgs();
    if (!probe.processCommandFiles(f.string(), /* makeRelative */ true, /* separateUnit */ false)) return false;
    return probe.sourceLoader.hasFiles();
}

std::string pyList(const std::vector<std::string>& items, const std::string& indent) {
    if (items.empty()) return "[]";
    std::string out = "[\n";
    for (const auto& i : items) out += indent + "    \"" + i + "\",\n";
    return out + indent + "]";
}

} // namespace

Discovery discover(const fs::path& rootIn, const std::vector<fs::path>& cliFiles) {
    Discovery d;
    std::error_code ec;
    d.root = fs::weakly_canonical(fs::absolute(rootIn, ec), ec);

    std::vector<fs::path> svFiles, filelists;
    fs::path uvmSrc; // a uvm_pkg.sv inside the project
    for (auto it = fs::recursive_directory_iterator(d.root, fs::directory_options::skip_permission_denied, ec);
         it != fs::recursive_directory_iterator() && svFiles.size() < kMaxFiles; it.increment(ec)) {
        if (ec) break;
        if (it->is_directory(ec)) {
            if (skipDir(it->path())) it.disable_recursion_pending();
            continue;
        }
        std::string ext = it->path().extension().string();
        if (ext == ".sv" || ext == ".svh" || ext == ".v" || ext == ".vh") {
            svFiles.push_back(it->path());
            if (it->path().filename() == "uvm_pkg.sv") uvmSrc = it->path().parent_path();
        } else if (ext == ".f") {
            filelists.push_back(it->path());
        }
    }
    std::sort(svFiles.begin(), svFiles.end());
    std::sort(filelists.begin(), filelists.end());
    d.svFiles = svFiles.size();

    // The UVM library: in the project tree, else $UVM_HOME.
    std::string uvmPkg, uvmInc;
    if (!uvmSrc.empty()) {
        fs::path home = uvmSrc.filename() == "src" ? uvmSrc.parent_path() : uvmSrc;
        uvmPkg = rel(uvmSrc / "uvm_pkg.sv", d.root);
        uvmInc = rel(uvmSrc, d.root);
        d.libraryRel.push_back(rel(home, d.root));
        d.uvmNote = "UVM library found in the project at " + rel(home, d.root);
    } else if (const char* env = std::getenv("UVM_HOME"); env && *env && fs::exists(fs::path(env) / "src" / "uvm_pkg.sv", ec)) {
        uvmPkg = "$UVM_HOME/src/uvm_pkg.sv";
        uvmInc = "$UVM_HOME/src";
        d.libraryRel.push_back("$UVM_HOME");
        d.uvmNote = "UVM library from $UVM_HOME (" + std::string(env) + ")";
    } else {
        d.uvmNote = "UVM library not found: set UVM_HOME, or add its uvm_pkg.sv to sources";
    }

    // Which files are `included by others, and from where.
    static const std::regex includeRe(R"(`include\s+"([^"]+)\")");
    std::set<std::string> includedNames;
    std::set<std::string> includeDirs;
    std::map<std::string, std::vector<fs::path>> byName;
    for (const auto& f : svFiles) byName[f.filename().string()].push_back(f);
    for (const auto& f : svFiles) {
        if (isUnder(f, uvmSrc)) continue;
        std::string text = readText(f);
        for (std::sregex_iterator m(text.begin(), text.end(), includeRe), end; m != end; ++m) {
            fs::path inc = (*m)[1].str();
            includedNames.insert(inc.filename().string());
            // The folder that makes `include "inc" resolve: the found file's
            // folder minus the include's own sub-path.
            auto found = byName.find(inc.filename().string());
            if (found == byName.end()) continue;
            for (const auto& cand : found->second) {
                if (isUnder(cand, uvmSrc)) continue;
                fs::path dir = cand.parent_path();
                for (auto p = inc.parent_path(); !p.empty() && p != p.root_path(); p = p.parent_path()) {
                    dir = dir.parent_path();
                }
                if (dir != f.parent_path()) includeDirs.insert(rel(dir, d.root));
            }
        }
    }

    if (!uvmPkg.empty()) d.sourcesRel.push_back(uvmPkg);
    if (!uvmInc.empty()) d.includeDirsRel.push_back(uvmInc);
    for (const auto& dir : includeDirs) {
        if (dir != uvmInc) d.includeDirsRel.push_back(dir);
    }

    if (!cliFiles.empty()) {
        for (const auto& f : cliFiles) d.sourcesRel.push_back(rel(fs::absolute(f, ec), d.root));
    } else {
        // File lists not named by another list are the tops; use them when
        // slang can read them, else fall back to the files themselves.
        std::set<std::string> nested;
        for (const auto& f : filelists) {
            std::string text = readText(f);
            for (const auto& g : filelists) {
                if (&g != &f && text.find(g.filename().string()) != std::string::npos) nested.insert(g.string());
            }
        }
        for (const auto& f : filelists) {
            if (nested.count(f.string())) continue;
            (usableFilelist(f) ? d.filelistsRel : d.unusedFilelists).push_back(rel(f, d.root));
        }
        if (d.filelistsRel.empty()) {
            for (const auto& f : svFiles) {
                if (isUnder(f, uvmSrc)) continue;
                if (includedNames.count(f.filename().string())) continue;
                std::string ext = f.extension().string();
                if (ext == ".svh" || ext == ".vh") continue; // headers only make sense when included
                d.sourcesRel.push_back(rel(f, d.root));
            }
        }
    }

    auto absolute = [&](const std::string& p) {
        std::string expanded;
        if (!expandEnvVars(p, expanded)) return std::string();
        fs::path fp(expanded);
        return (fp.is_relative() ? d.root / fp : fp).lexically_normal().generic_string();
    };
    for (const auto& s : d.sourcesRel) d.build.sources.push_back(absolute(s));
    for (const auto& s : d.filelistsRel) d.build.filelists.push_back(absolute(s));
    for (const auto& s : d.includeDirsRel) d.build.includeDirs.push_back(absolute(s));
    return d;
}

std::vector<NamingChoice> inferNaming(const uvm::UvmModel& model, style::NamingEngine& engine,
                                      const fs::path& libraryDir) {
    // Class names per UVM kind, user code only.
    std::map<std::string, std::vector<std::string>> names;
    const slang::SourceManager* sm = nullptr;
    for (const auto* cls : rules::swe::userClasses(model)) {
        if (!sm) sm = cls->getCompilation().getSourceManager();
        if (sm && cls->location.valid() && !libraryDir.empty()) {
            fs::path file = sm->getFullPath(sm->getFullyOriginalLoc(cls->location).buffer());
            if (isUnder(file.lexically_normal(), libraryDir)) continue;
        }
        std::string kind = style::NamingEngine::classTargetKind(model, *cls);
        if (kind.rfind("Uvm", 0) != 0) continue;
        names[kind].emplace_back(cls->name);
    }

    std::vector<NamingChoice> out;
    for (auto& [kind, list] : names) {
        const auto* spec = engine.getRule(kind);
        if (!spec || !spec->enabled || list.size() < 2) continue;
        size_t pass = 0;
        std::string reason, suggested;
        for (const auto& n : list) pass += engine.validate(kind, n, reason, suggested) ? 1 : 0;
        if (pass * 10 >= list.size() * 8) continue; // 80% already follow the default

        // Group by ending: "_drv" for snake_case, the last capitalised word for PascalCase.
        std::map<std::string, size_t> endings;
        for (const auto& n : list) {
            std::string e;
            if (size_t u = n.rfind('_'); u != std::string::npos && u > 0) {
                e = n.substr(u);
            } else {
                size_t i = n.size();
                while (i > 1 && !std::isupper(static_cast<unsigned char>(n[i - 1]))) --i;
                e = i > 1 ? n.substr(i - 1) : std::string();
            }
            endings[e]++;
        }
        std::vector<std::pair<std::string, size_t>> sorted(endings.begin(), endings.end());
        std::sort(sorted.begin(), sorted.end(), [](auto& a, auto& b) { return a.second > b.second; });

        std::vector<std::string> chosen;
        size_t covered = 0;
        for (const auto& [e, n] : sorted) {
            if (e.empty() || n < 2 || chosen.size() == 3) break;
            chosen.push_back(e);
            covered += n;
            if (covered * 10 >= list.size() * 8) break;
        }

        NamingChoice c;
        c.target = kind;
        std::ostringstream ev;
        if (!chosen.empty() && covered * 10 >= list.size() * 8) {
            std::string alt;
            for (const auto& e : chosen) alt += (alt.empty() ? "" : "|") + e;
            c.pattern = "^[A-Za-z0-9_]*(" + alt + ")$";
            ev << covered << " of " << list.size() << " classes end in ";
            for (size_t i = 0; i < chosen.size(); ++i) ev << (i ? " or " : "") << chosen[i];
        } else {
            c.disable = true;
            ev << "no common ending across " << list.size() << " classes (top: "
               << (sorted.empty() ? "none" : sorted.front().first + " x" + std::to_string(sorted.front().second)) << ")";
        }
        ev << "; " << pass << " follow the default " << spec->regexPattern;
        c.evidence = ev.str();

        style::NamingTargetSpec updated = *spec;
        if (c.disable) {
            updated.enabled = false;
        } else {
            updated.regexPattern = c.pattern;
            updated.prefix.clear();
            updated.suffix.clear();
        }
        engine.setRule(kind, updated);
        out.push_back(std::move(c));
    }
    return out;
}

std::string renderConfig(const Discovery& d, const std::vector<NamingChoice>& naming) {
    std::ostringstream os;
    os << R"PY("""UVMCheck project configuration, written by `uvmcheck --init`.

Paths are relative to the project root (the folder holding .uvmcheck/).
See docs/guide/project_config.md for every setting.
"""
from uvmcheck.config import ProjectConfig, RuleConfig, Waiver, Severity


class Project(ProjectConfig):
)PY";
    os << "    # " << d.uvmNote << ". " << d.svFiles << " SystemVerilog files scanned.\n";
    if (!d.filelistsRel.empty()) {
        os << "    filelists = " << pyList(d.filelistsRel, "    ") << "\n";
    }
    for (const auto& f : d.unusedFilelists) {
        os << "    # Found " << f << ", but it did not load as a slang file list (simulator-only options?).\n";
    }
    os << "    sources = " << pyList(d.sourcesRel, "    ") << "\n";
    os << "    include_dirs = " << pyList(d.includeDirsRel, "    ") << "\n";
    os << "    defines = []\n\n";
    os << "    # Third-party code: compiled so the testbench elaborates, never reported.\n";
    os << "    library_paths = " << pyList(d.libraryRel, "    ") << "\n";

    if (!naming.empty()) {
        os << "\n    # Naming conventions learnt from the code (default kept where 80% already follow it).\n";
        os << "    naming = {\n";
        for (const auto& n : naming) {
            os << "        # " << n.evidence << "\n";
            if (n.disable) {
                os << "        \"" << n.target << "\": {\"enabled\": False},\n";
            } else {
                os << "        \"" << n.target << "\": {\"pattern\": r\"" << n.pattern
                   << "\", \"prefix\": \"\", \"suffix\": \"\"},\n";
            }
        }
        os << "    }\n";
    }
    os << R"PY(
    # The rule set: "default" finds bugs and methodology problems; "all" adds
    # style, maintainability and advisory rules. A rule configured below
    # runs whatever the preset.
    # preset = "all"

    # Per-rule settings, e.g.:
    # class MaxLineLength(RuleConfig):
    #     max_columns = 120

    waivers = [
        # Waiver("UVM-OBJ-002", path="tb/stress_seq.sv", line=142,
        #        reason="...", owner="you", expires="2027-06-30"),
    ]
)PY";
    return os.str();
}

} // namespace uvmcheck::core::init
