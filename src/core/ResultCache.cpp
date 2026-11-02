#include "uvmcheck/core/ResultCache.h"
#include "uvmcheck/core/Process.h"

#include "uvmcheck/core/Json.h"
#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"

#include "slang/ast/ASTVisitor.h"
#include "slang/ast/Compilation.h"
#include "slang/ast/symbols/ClassSymbols.h"
#include "slang/ast/symbols/CompilationUnitSymbols.h"
#include "slang/ast/symbols/MemberSymbols.h"
#include "slang/syntax/SyntaxTree.h"

#include <algorithm>
#include <cstdio>
#include <fstream>
#include <map>
#include <set>
#include <sstream>
#include <unordered_map>
#include <unordered_set>

namespace uvmcheck::core::cache {

namespace {

constexpr uint64_t kFnvA = 14695981039346656037ull;
constexpr uint64_t kFnvB = 0x6C62272E07BB0142ull;
constexpr uint64_t kPrime = 1099511628211ull;

struct Hasher {
    uint64_t a = kFnvA;
    uint64_t b = kFnvB;

    void add(std::string_view s) {
        a ^= s.size();
        a *= kPrime;
        b ^= s.size() * 0x9E3779B185EBCA87ull;
        for (unsigned char c : s) {
            a ^= c;
            a *= kPrime;
            b ^= c;
            b *= kPrime;
        }
    }

    std::string hex() const {
        static constexpr char kDig[] = "0123456789abcdef";
        std::string out(32, '0');
        for (int i = 0; i < 16; i++) {
            uint64_t v = (i < 8) ? (a >> ((7 - i) * 8)) : (b >> ((15 - i) * 8));
            out[static_cast<size_t>(i * 2)] = kDig[(v >> 4) & 0xF];
            out[static_cast<size_t>(i * 2 + 1)] = kDig[v & 0xF];
        }
        return out;
    }
};

std::string escapeJson(std::string_view s) {
    std::string out;
    out.reserve(s.size());
    for (char c : s) {
        switch (c) {
            case '"': out += "\\\""; break;
            case '\\': out += "\\\\"; break;
            case '\n': out += "\\n"; break;
            case '\r': out += "\\r"; break;
            case '\t': out += "\\t"; break;
            default:
                if (static_cast<unsigned char>(c) < 0x20) {
                    char buf[8];
                    std::snprintf(buf, sizeof(buf), "\\u%04x", static_cast<unsigned>(static_cast<unsigned char>(c)));
                    out += buf;
                } else {
                    out += c;
                }
        }
    }
    return out;
}

std::string canonicalPath(const std::filesystem::path& path) {
    std::error_code ec;
    std::filesystem::path c = std::filesystem::weakly_canonical(path, ec);
    if (ec || c.empty()) c = std::filesystem::absolute(path, ec).lexically_normal();
    return c.generic_string();
}

std::string readFile(const std::filesystem::path& path, bool& ok) {
    std::ifstream in(path, std::ios::binary);
    if (!in) {
        ok = false;
        return {};
    }
    return std::string(std::istreambuf_iterator<char>(in), std::istreambuf_iterator<char>());
}

std::string hashBytes(std::string_view bytes) {
    Hasher h;
    h.add(bytes);
    return h.hex();
}

/// Empty when the file cannot be read.
std::string hashFile(const std::filesystem::path& path) {
    bool ok = true;
    std::string bytes = readFile(path, ok);
    if (!ok) return {};
    return hashBytes(bytes);
}

const std::unordered_set<std::string_view>& wholeProgramIds() {
    // A finding of one of these can appear or disappear because a file changed
    // that the reported file does not include and does not import.
    static const std::unordered_set<std::string_view> ids = {
        "SV-DUP-001", "SV-DUP-002", "SV-DUP-003", "SV-DUP-004", "SV-DUP-005", "SV-DUP-006",
        "SV-DED-015", "SV-PKG-004", "SV-PKG-005", "SV-EVO-001", "SV-EVO-002",
        "UVM-ARC-001", "UVM-ARC-003",
        "UVM-TLM-001", "UVM-TLM-004", "UVM-VSEQ-001", "UVM-SEQ-013", "UVM-SEQ-014", "UVM-SEQ-015",
        "UVM-CFG-005", "UVM-CFG-010", "UVM-CFG-011", "UVM-PHS-010",
        "RAL-WIRE-001", "RAL-MAP-001", "RAL-MAP-002", "RAL-COL-001",
    };
    return ids;
}

struct Dep {
    std::string path;
    std::string hash;
};

struct Unit {
    std::string primary;
    std::string key;
    std::vector<Dep> deps;
    bool sharesUnit = false;
};

struct Manifest {
    std::string env;
    std::vector<Unit> units;
    std::string wholeKey;
    std::vector<Dep> inputs; // union of unit deps
    std::string mergedKey;
};

std::string depKey(const std::string& env, const std::vector<Dep>& deps) {
    Hasher h;
    h.add(env);
    for (const auto& d : deps) {
        h.add(d.path);
        h.add(d.hash);
    }
    return h.hex();
}

bool writeText(const std::filesystem::path& path, const std::string& body) {
    std::error_code ec;
    std::filesystem::create_directories(path.parent_path(), ec);
    std::filesystem::path tmp = path;
    tmp += ".tmp";
    {
        std::ofstream out(tmp, std::ios::binary | std::ios::trunc);
        if (!out) return false;
        out << body;
        if (!out) return false;
    }
    std::filesystem::rename(tmp, path, ec);
    return !ec;
}

void writeViolation(std::ostream& os, const Violation& v) {
    os << "{\"ruleId\":\"" << escapeJson(v.ruleId) << "\",\"ruleName\":\"" << escapeJson(v.ruleName)
       << "\",\"message\":\"" << escapeJson(v.message) << "\",\"severity\":\"" << severityToString(v.severity)
       << "\",\"file\":\"" << escapeJson(v.filePath) << "\",\"line\":" << v.line << ",\"column\":" << v.column
       << ",\"endColumn\":" << v.endColumn << ",\"snippet\":\"" << escapeJson(v.snippet) << "\"";
    if (!v.fixIts.empty()) {
        os << ",\"fixes\":[";
        for (size_t i = 0; i < v.fixIts.size(); i++) {
            const auto& f = v.fixIts[i];
            if (i) os << ',';
            os << "{\"description\":\"" << escapeJson(f.description) << "\",\"replacement\":\""
               << escapeJson(f.replacementText) << "\",\"safety\":\"" << fixSafetyToString(f.safety)
               << "\",\"line\":" << f.startLine << ",\"column\":" << f.startColumn << ",\"endLine\":" << f.endLine
               << ",\"endColumn\":" << f.endColumn << "}";
        }
        os << ']';
    }
    if (!v.related.empty()) {
        os << ",\"related\":[";
        for (size_t i = 0; i < v.related.size(); i++) {
            const auto& r = v.related[i];
            if (i) os << ',';
            os << "{\"file\":\"" << escapeJson(r.filePath) << "\",\"line\":" << r.line << ",\"column\":" << r.column
               << ",\"message\":\"" << escapeJson(r.message) << "\"}";
        }
        os << ']';
    }
    os << '}';
}

std::string violationsBlob(const std::vector<Violation>& violations, size_t compileErrors, size_t waived) {
    std::ostringstream os;
    os << "{\"format\":" << kFormatVersion << ",\"compileErrors\":" << compileErrors << ",\"waived\":" << waived
       << ",\"violations\":[";
    for (size_t i = 0; i < violations.size(); i++) {
        if (i) os << ',';
        writeViolation(os, violations[i]);
    }
    os << "]}";
    return os.str();
}

std::optional<size_t> numField(const json::Value& obj, std::string_view key) {
    const json::Value* v = obj.find(key);
    if (!v || !v->isNumber() || v->asNumber() < 0) return std::nullopt;
    return static_cast<size_t>(v->asNumber());
}

std::optional<Violation> readViolation(const json::Value& v) {
    if (!v.isObject()) return std::nullopt;
    auto str = [&](std::string_view key) -> const std::string* {
        const json::Value* f = v.find(key);
        return f && f->isString() ? &f->asString() : nullptr;
    };
    const std::string* ruleId = str("ruleId");
    const std::string* ruleName = str("ruleName");
    const std::string* message = str("message");
    const std::string* severity = str("severity");
    const std::string* file = str("file");
    const std::string* snippet = str("snippet");
    auto line = numField(v, "line");
    auto column = numField(v, "column");
    auto endColumn = numField(v, "endColumn");
    if (!ruleId || !ruleName || !message || !severity || !file || !snippet || !line || !column || !endColumn)
        return std::nullopt;
    Violation out;
    out.ruleId = *ruleId;
    out.ruleName = *ruleName;
    out.message = *message;
    out.filePath = *file;
    out.snippet = *snippet;
    out.line = *line;
    out.column = *column;
    out.endColumn = *endColumn;
    if (*severity == "ERROR") out.severity = Severity::Error;
    else if (*severity == "WARNING") out.severity = Severity::Warning;
    else if (*severity == "INFO") out.severity = Severity::Info;
    else return std::nullopt;
    if (const json::Value* fixes = v.find("fixes"); fixes && fixes->isArray()) {
        for (const auto& f : fixes->asArray()) {
            if (!f.isObject()) return std::nullopt;
            const json::Value* desc = f.find("description");
            const json::Value* repl = f.find("replacement");
            const json::Value* safety = f.find("safety");
            auto fl = numField(f, "line");
            auto fc = numField(f, "column");
            auto fel = numField(f, "endLine");
            auto fec = numField(f, "endColumn");
            if (!desc || !desc->isString() || !repl || !repl->isString() || !safety || !safety->isString() || !fl ||
                !fc || !fel || !fec)
                return std::nullopt;
            FixItHint hint;
            hint.description = desc->asString();
            hint.replacementText = repl->asString();
            hint.safety = fixSafetyFromString(safety->asString());
            hint.startLine = *fl;
            hint.startColumn = *fc;
            hint.endLine = *fel;
            hint.endColumn = *fec;
            out.fixIts.push_back(std::move(hint));
        }
    }
    if (const json::Value* related = v.find("related"); related && related->isArray()) {
        for (const auto& r : related->asArray()) {
            if (!r.isObject()) return std::nullopt;
            const json::Value* rf = r.find("file");
            const json::Value* rm = r.find("message");
            auto rl = numField(r, "line");
            auto rc = numField(r, "column");
            if (!rf || !rf->isString() || !rm || !rm->isString() || !rl || !rc) return std::nullopt;
            out.related.push_back({rf->asString(), *rl, *rc, rm->asString()});
        }
    }
    return out;
}

struct Blob {
    std::vector<Violation> violations;
    size_t compileErrors = 0;
    size_t waived = 0;
};

std::optional<Blob> readBlob(const std::filesystem::path& path) {
    bool ok = true;
    std::string text = readFile(path, ok);
    if (!ok) return std::nullopt;
    json::ParseError err;
    auto root = json::parse(text, err);
    if (!root || !root->isObject()) return std::nullopt;
    auto format = numField(*root, "format");
    auto compileErrors = numField(*root, "compileErrors");
    auto waived = numField(*root, "waived");
    const json::Value* list = root->find("violations");
    if (!format || *format != static_cast<size_t>(kFormatVersion) || !compileErrors || !waived || !list ||
        !list->isArray())
        return std::nullopt;
    Blob blob;
    blob.compileErrors = *compileErrors;
    blob.waived = *waived;
    for (const auto& item : list->asArray()) {
        auto v = readViolation(item);
        if (!v) return std::nullopt;
        blob.violations.push_back(std::move(*v));
    }
    return blob;
}

std::optional<Manifest> readManifest(const std::filesystem::path& path) {
    bool ok = true;
    std::string text = readFile(path, ok);
    if (!ok) return std::nullopt;
    json::ParseError err;
    auto root = json::parse(text, err);
    if (!root || !root->isObject()) return std::nullopt;
    auto format = numField(*root, "format");
    const json::Value* tool = root->find("tool");
    const json::Value* env = root->find("env");
    const json::Value* units = root->find("units");
    const json::Value* whole = root->find("wholeProgram");
    const json::Value* merged = root->find("merged");
    if (!format || *format != static_cast<size_t>(kFormatVersion) || !tool || !tool->isString() ||
        tool->asString() != kToolVersion || !env || !env->isString() || !units || !units->isArray() || !whole ||
        !whole->isObject() || !merged || !merged->isString())
        return std::nullopt;
    Manifest m;
    m.env = env->asString();
    m.mergedKey = merged->asString();
    const json::Value* wk = whole->find("key");
    const json::Value* inputs = whole->find("inputs");
    if (!wk || !wk->isString() || !inputs || !inputs->isArray()) return std::nullopt;
    m.wholeKey = wk->asString();
    auto readDeps = [](const json::Value& arr, std::vector<Dep>& out) {
        if (!arr.isArray()) return false;
        for (const auto& d : arr.asArray()) {
            if (!d.isObject()) return false;
            const json::Value* p = d.find("path");
            const json::Value* h = d.find("hash");
            if (!p || !p->isString() || !h || !h->isString()) return false;
            out.push_back({p->asString(), h->asString()});
        }
        return true;
    };
    if (!readDeps(*inputs, m.inputs)) return std::nullopt;
    for (const auto& u : units->asArray()) {
        if (!u.isObject()) return std::nullopt;
        const json::Value* primary = u.find("primary");
        const json::Value* key = u.find("key");
        const json::Value* deps = u.find("deps");
        if (!primary || !primary->isString() || !key || !key->isString() || !deps) return std::nullopt;
        Unit unit;
        unit.primary = primary->asString();
        unit.key = key->asString();
        if (!readDeps(*deps, unit.deps)) return std::nullopt;
        m.units.push_back(std::move(unit));
    }
    return m;
}

std::string manifestText(const Manifest& m) {
    std::ostringstream os;
    os << "{\"format\":" << kFormatVersion << ",\"tool\":\"" << kToolVersion << "\",\"env\":\"" << m.env
       << "\",\"merged\":\"" << m.mergedKey << "\",\"units\":[";
    for (size_t i = 0; i < m.units.size(); i++) {
        const auto& u = m.units[i];
        if (i) os << ',';
        os << "{\"primary\":\"" << escapeJson(u.primary) << "\",\"key\":\"" << u.key << "\",\"deps\":[";
        for (size_t d = 0; d < u.deps.size(); d++) {
            if (d) os << ',';
            os << "{\"path\":\"" << escapeJson(u.deps[d].path) << "\",\"hash\":\"" << u.deps[d].hash << "\"}";
        }
        os << "]}";
    }
    os << "],\"wholeProgram\":{\"key\":\"" << m.wholeKey << "\",\"inputs\":[";
    for (size_t d = 0; d < m.inputs.size(); d++) {
        if (d) os << ',';
        os << "{\"path\":\"" << escapeJson(m.inputs[d].path) << "\",\"hash\":\"" << m.inputs[d].hash << "\"}";
    }
    os << "]}}";
    return os.str();
}

std::filesystem::path blobPath(const std::filesystem::path& dir, char kind, const std::string& key) {
    return dir / std::string(1, kind) / (key + ".json");
}

bool depsMatch(const std::vector<Dep>& recorded) {
    for (const auto& d : recorded) {
        if (hashFile(d.path) != d.hash) return false;
    }
    return true;
}

std::string fileOf(const slang::SourceManager& sm, slang::SourceLocation loc) {
    if (!loc) return {};
    loc = sm.getFullyOriginalLoc(loc);
    if (!loc.valid() || !sm.isFileLoc(loc)) return {};
    const std::filesystem::path& full = sm.getFullPath(loc.buffer());
    if (full.empty()) return {};
    return canonicalPath(full);
}

using namespace slang::ast;

class ImportScan : public ASTVisitor<ImportScan, VisitFlags::AllGood> {
public:
    explicit ImportScan(const slang::SourceManager& sm) : sm(sm) {}

    void handle(const WildcardImportSymbol& s) { add(s, s.packageName); }
    void handle(const ExplicitImportSymbol& s) { add(s, s.packageName); }
    void handle(const ClassType&) {}
    void handle(const GenericClassDefSymbol&) {}
    void handle(const SubroutineSymbol&) {}
    void handle(const ProceduralBlockSymbol&) {}

    std::map<std::string, std::set<std::string>> byFile;

private:
    const slang::SourceManager& sm;

    void add(const Symbol& s, std::string_view package) {
        if (package.empty() || package == "std") return;
        std::string file = fileOf(sm, s.location);
        if (!file.empty()) byFile[file].insert(std::string(package));
    }
};

void addPackageFiles(const PackageSymbol& pkg, const slang::SourceManager& sm, std::set<std::string>& files,
                     std::set<std::string>& morePackages) {
    auto addLoc = [&](slang::SourceLocation loc) {
        std::string f = fileOf(sm, loc);
        if (!f.empty()) files.insert(std::move(f));
    };
    addLoc(pkg.location);
    for (const auto& member : pkg.members()) {
        addLoc(member.location);
        if (member.kind == SymbolKind::WildcardImport)
            morePackages.insert(std::string(member.as<WildcardImportSymbol>().packageName));
        else if (member.kind == SymbolKind::ExplicitImport)
            morePackages.insert(std::string(member.as<ExplicitImportSymbol>().packageName));
    }
}

std::vector<Unit> discoverUnits(Compilation& comp) {
    const auto* sm = comp.getSourceManager();
    if (!sm) return {};

    std::map<std::string, Unit> byPrimary;
    for (const auto& tree : comp.getSyntaxTrees()) {
        if (!tree) continue;
        std::string primary;
        std::set<std::string> files;
        for (auto id : tree->getSourceBufferIds()) {
            slang::SourceLocation origin(id, 0);
            std::string path = fileOf(*sm, origin);
            if (path.empty()) continue;
            files.insert(path);
            if (!sm->isIncludedFileLoc(origin)) primary = path;
        }
        if (primary.empty()) continue;
        auto& unit = byPrimary[primary];
        unit.primary = primary;
        for (const auto& f : files) {
            if (std::find_if(unit.deps.begin(), unit.deps.end(), [&](const Dep& d) { return d.path == f; }) ==
                unit.deps.end())
                unit.deps.push_back({f, {}});
        }
    }

    ImportScan scan(*sm);
    comp.getRoot().visit(scan);
    for (const auto* pkg : comp.getPackages()) {
        if (pkg) pkg->visit(scan);
    }

    for (auto& [primary, unit] : byPrimary) {
        std::set<std::string> files;
        for (const auto& d : unit.deps) files.insert(d.path);
        std::set<std::string> seenPkgs;
        std::vector<std::string> queue;
        auto enqueueFrom = [&](const std::string& file) {
            auto it = scan.byFile.find(file);
            if (it == scan.byFile.end()) return;
            for (const auto& name : it->second)
                if (seenPkgs.insert(name).second) queue.push_back(name);
        };
        for (const auto& f : files) enqueueFrom(f);
        for (size_t i = 0; i < queue.size(); i++) {
            if (queue[i].empty() || queue[i] == "std") continue;
            const auto* pkg = comp.getPackage(queue[i]);
            if (!pkg) continue;
            std::set<std::string> more;
            std::set<std::string> added;
            addPackageFiles(*pkg, *sm, added, more);
            for (const auto& f : added) {
                if (files.insert(f).second) enqueueFrom(f);
            }
            for (const auto& name : more)
                if (seenPkgs.insert(name).second) queue.push_back(name);
        }
        unit.deps.clear();
        for (const auto& f : files) unit.deps.push_back({f, {}});
        (void)primary;
    }

    for (const auto* cu : comp.getCompilationUnits()) {
        if (!cu) continue;
        for (const auto& member : cu->members()) {
            if (member.kind == SymbolKind::Package) continue;
            std::string file = fileOf(*sm, member.location);
            auto it = byPrimary.find(file);
            if (it != byPrimary.end()) it->second.sharesUnit = true;
        }
    }
    std::set<std::string> sharedDeps;
    for (const auto& [_, unit] : byPrimary) {
        if (!unit.sharesUnit) continue;
        for (const auto& d : unit.deps) sharedDeps.insert(d.path);
    }
    for (auto& [_, unit] : byPrimary) {
        if (!unit.sharesUnit) continue;
        for (const auto& f : sharedDeps) {
            if (std::find_if(unit.deps.begin(), unit.deps.end(), [&](const Dep& d) { return d.path == f; }) ==
                unit.deps.end())
                unit.deps.push_back({f, {}});
        }
        std::sort(unit.deps.begin(), unit.deps.end(), [](const Dep& a, const Dep& b) { return a.path < b.path; });
    }

    std::vector<Unit> units;
    for (auto& [_, unit] : byPrimary) {
        std::sort(unit.deps.begin(), unit.deps.end(), [](const Dep& a, const Dep& b) { return a.path < b.path; });
        units.push_back(std::move(unit));
    }
    std::sort(units.begin(), units.end(), [](const Unit& a, const Unit& b) { return a.primary < b.primary; });
    return units;
}

bool fillHashes(std::vector<Dep>& deps) {
    for (auto& d : deps) {
        d.hash = hashFile(d.path);
        if (d.hash.empty() && !std::filesystem::is_regular_file(d.path)) return false;
        if (d.hash.empty()) return false;
    }
    return true;
}

std::string baseName(const std::string& path) {
    auto slash = path.find_last_of('/');
    return slash == std::string::npos ? path : path.substr(slash + 1);
}

void sortViolations(std::vector<Violation>& violations) {
    std::stable_sort(violations.begin(), violations.end(), [](const Violation& a, const Violation& b) {
        return std::tie(a.filePath, a.line, a.column, a.ruleId, a.message) <
               std::tie(b.filePath, b.line, b.column, b.ruleId, b.message);
    });
}

} // namespace

bool isWholeProgramRule(std::string_view ruleId) {
    return wholeProgramIds().count(ruleId) != 0;
}

std::string buildIdentity() {
    // The version string does not change between builds, so a rebuilt binary (new or
    // changed rules, different fix text) must not reuse findings from the old one. The
    // executable's size and modification time change on every relink.
    static const std::string id = [] {
        const std::string exe = selfExecutablePath("");
        std::error_code ec;
        if (exe.empty() || !std::filesystem::is_regular_file(exe, ec)) return std::string("unknown");
        const auto size = std::filesystem::file_size(exe, ec);
        if (ec) return std::string("unknown");
        const auto mtime = std::filesystem::last_write_time(exe, ec);
        if (ec) return std::string("unknown");
        return std::to_string(size) + "-" + std::to_string(static_cast<long long>(mtime.time_since_epoch().count()));
    }();
    return id;
}

std::string environmentKey(const EnvInput& in) {
    Hasher h;
    h.add(kToolVersion);
    h.add(buildIdentity());
    h.add(std::to_string(kFormatVersion));
    h.add(in.languageVersion);
    h.add(in.disableLocalIncludes ? "1" : "0");
    h.add(in.incDirFirst ? "1" : "0");
    std::vector<std::string> defines = in.defines;
    std::vector<std::string> undefines = in.undefines;
    std::vector<std::string> inc = in.includeDirs;
    std::sort(defines.begin(), defines.end());
    std::sort(undefines.begin(), undefines.end());
    std::sort(inc.begin(), inc.end());
    for (const auto& d : defines) h.add(d);
    h.add("|");
    for (const auto& d : undefines) h.add(d);
    h.add("|");
    for (const auto& d : inc) h.add(d);
    h.add("|");
    h.add(in.flags);
    // A folder (--python-rules <dir>) stands for the files under it.
    std::vector<std::string> files;
    for (const auto& f : in.contentFiles) {
        std::error_code ec;
        if (std::filesystem::is_directory(f, ec)) {
            for (const auto& e : std::filesystem::recursive_directory_iterator(f, std::filesystem::directory_options::skip_permission_denied, ec)) {
                if (e.is_regular_file(ec) && e.path().extension() == ".py") files.push_back(e.path().generic_string());
            }
        } else {
            files.push_back(f);
        }
    }
    std::sort(files.begin(), files.end());
    files.erase(std::unique(files.begin(), files.end()), files.end());
    for (const auto& f : files) {
        h.add(f);
        bool ok = true;
        h.add(readFile(f, ok));
        if (!ok) h.add("missing");
    }
    std::vector<std::string> rules;
    for (const auto& r : rules::RuleRegistry::instance().getRules()) {
        std::string row(r->getId());
        row.push_back('\n');
        row += r->getName();
        row.push_back('\n');
        row += severityToString(r->getDefaultSeverity());
        row.push_back('\n');
        row += isWholeProgramRule(r->getId()) ? "W" : "L";
        row += r->getMaturity() == rules::RuleMaturity::Experimental ? "E" : "S";
        rules.push_back(std::move(row));
    }
    std::sort(rules.begin(), rules.end());
    for (const auto& row : rules) h.add(row);
    return h.hex();
}

void clear(const std::filesystem::path& directory) {
    std::error_code ec;
    std::filesystem::remove_all(directory, ec);
}

std::optional<Hit> loadComplete(const Request& req) {
    try {
        if (!req.enabled || req.envKey.empty()) return std::nullopt;
        auto manifest = readManifest(req.directory / "manifest.json");
        if (!manifest || manifest->env != req.envKey) return std::nullopt;
        std::vector<std::string> primaries;
        for (const auto& u : manifest->units) primaries.push_back(u.primary);
        if (primaries != req.primaries) return std::nullopt;
        for (const auto& u : manifest->units) {
            if (!depsMatch(u.deps)) return std::nullopt;
        }
        if (!depsMatch(manifest->inputs)) return std::nullopt;
        auto blob = readBlob(blobPath(req.directory, 'm', manifest->mergedKey));
        if (!blob) return std::nullopt;
        Hit hit;
        hit.violations = std::move(blob->violations);
        hit.compileErrors = blob->compileErrors;
        hit.waivedCount = blob->waived;
        return hit;
    }
    catch (...) {
        return std::nullopt;
    }
}

void updateAndStore(const Request& req, slang::ast::Compilation& compilation,
                    std::vector<Violation>& violations, size_t compileErrors, size_t waivedCount,
                    std::ostream& info) {
    try {
        if (!req.enabled || req.envKey.empty()) return;
        std::vector<Unit> units = discoverUnits(compilation);
        if (units.empty()) return;
        for (auto& u : units) {
            if (!fillHashes(u.deps)) return;
            u.key = depKey(req.envKey, u.deps);
        }
        std::map<std::string, const Unit*> owner;
        for (const auto& u : units) owner[u.primary] = &u;
        for (const auto& u : units) {
            for (const auto& d : u.deps)
                if (!owner.count(d.path)) owner.emplace(d.path, &u);
        }
        std::set<std::string> inputPaths;
        for (const auto& u : units)
            for (const auto& d : u.deps) inputPaths.insert(d.path);
        std::vector<Dep> inputs;
        for (const auto& p : inputPaths) inputs.push_back({p, hashFile(p)});
        for (const auto& d : inputs)
            if (d.hash.empty()) return;
        std::string wholeKey = depKey(req.envKey, inputs);

        std::optional<Manifest> previous;
        try {
            previous = readManifest(req.directory / "manifest.json");
        }
        catch (...) {
            previous.reset();
        }
        const bool sameEnv = previous && previous->env == req.envKey;
        std::unordered_map<std::string, const Unit*> oldByPrimary;
        if (sameEnv) {
            for (const auto& u : previous->units) oldByPrimary[u.primary] = &u;
        }
        const bool wholeHit = sameEnv && previous->wholeKey == wholeKey && depsMatch(previous->inputs);

        std::unordered_map<std::string, std::vector<Violation>> cachedLocal;
        std::set<std::string> hitPrimaries;
        std::vector<std::string> misses;
        for (const auto& u : units) {
            const Unit* old = nullptr;
            if (sameEnv) {
                auto it = oldByPrimary.find(u.primary);
                if (it != oldByPrimary.end()) old = it->second;
            }
            bool hit = old && old->key == u.key && depsMatch(old->deps);
            if (hit) {
                auto blob = readBlob(blobPath(req.directory, 'u', old->key));
                if (!blob) hit = false;
                else cachedLocal[u.primary] = std::move(blob->violations);
            }
            if (hit) hitPrimaries.insert(u.primary);
            else misses.push_back(baseName(u.primary));
        }

        if (sameEnv && (!misses.empty() || !wholeHit) && !hitPrimaries.empty()) {
            std::vector<Violation> freshWp, freshLocalUnowned;
            std::unordered_map<std::string, std::vector<Violation>> freshLocal;
            for (auto& v : violations) {
                if (isWholeProgramRule(v.ruleId)) {
                    freshWp.push_back(std::move(v));
                    continue;
                }
                std::string file = v.filePath.empty() ? std::string() : canonicalPath(v.filePath);
                auto it = owner.find(file);
                if (it == owner.end()) freshLocalUnowned.push_back(std::move(v));
                else freshLocal[it->second->primary].push_back(std::move(v));
            }
            std::vector<Violation> merged = std::move(freshLocalUnowned);
            for (const auto& u : units) {
                auto& src = hitPrimaries.count(u.primary) ? cachedLocal[u.primary] : freshLocal[u.primary];
                for (auto& v : src) merged.push_back(std::move(v));
            }
            if (wholeHit) {
                if (auto blob = readBlob(blobPath(req.directory, 'w', previous->wholeKey))) {
                    for (auto& v : blob->violations) merged.push_back(std::move(v));
                } else {
                    for (auto& v : freshWp) merged.push_back(std::move(v));
                }
            } else {
                for (auto& v : freshWp) merged.push_back(std::move(v));
            }
            sortViolations(merged);
            violations = std::move(merged);
        }

        std::unordered_map<std::string, std::vector<Violation>> local;
        std::vector<Violation> whole;
        for (const auto& v : violations) {
            if (isWholeProgramRule(v.ruleId)) {
                whole.push_back(v);
                continue;
            }
            std::string file = v.filePath.empty() ? std::string() : canonicalPath(v.filePath);
            auto it = owner.find(file);
            if (it != owner.end()) local[it->second->primary].push_back(v);
        }
        for (const auto& u : units) {
            if (!writeText(blobPath(req.directory, 'u', u.key), violationsBlob(local[u.primary], 0, 0))) return;
        }
        if (!writeText(blobPath(req.directory, 'w', wholeKey), violationsBlob(whole, 0, 0))) return;

        Hasher mergedHash;
        mergedHash.add(req.envKey);
        mergedHash.add(wholeKey);
        for (const auto& u : units) mergedHash.add(u.key);
        std::string mergedKey = mergedHash.hex();
        if (!writeText(blobPath(req.directory, 'm', mergedKey), violationsBlob(violations, compileErrors, waivedCount)))
            return;

        Manifest manifest;
        manifest.env = req.envKey;
        manifest.units = units;
        manifest.wholeKey = wholeKey;
        manifest.inputs = std::move(inputs);
        manifest.mergedKey = mergedKey;
        if (!writeText(req.directory / "manifest.json", manifestText(manifest))) return;

        if (!sameEnv || hitPrimaries.empty()) {
            info << "[uvmcheck] cache: stored " << units.size() << " compilation unit"
                 << (units.size() == 1 ? "" : "s") << "\n";
        } else {
            for (const auto& primary : hitPrimaries) info << "[uvmcheck] cache hit: " << baseName(primary) << "\n";
            for (const auto& name : misses) info << "[uvmcheck] cache miss: " << name << "\n";
            if (!wholeHit) info << "[uvmcheck] cache miss: whole-program rules\n";
        }
    }
    catch (const std::exception& ex) {
        info << "[uvmcheck] cache: ignored (" << ex.what() << "); results are from this run\n";
    }
    catch (...) {
        info << "[uvmcheck] cache: ignored (unreadable entry); results are from this run\n";
    }
}

} // namespace uvmcheck::core::cache
