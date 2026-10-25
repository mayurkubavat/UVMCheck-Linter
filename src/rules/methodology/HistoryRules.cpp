// Methodology guide MG-9.2, change history: SV-EVO-001 (hotspots) and SV-EVO-002 (change
// coupling). Both read git through rules::history and do nothing unless --history is given.

#include "MethodologySupport.h"
#include "uvmcheck/rules/CodeMetrics.h"
#include "uvmcheck/rules/History.h"
#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"

#include "slang/ast/symbols/CompilationUnitSymbols.h"

#include <algorithm>
#include <map>
#include <set>

namespace uvmcheck::rules::methodology {

using namespace slang::ast;

namespace {

std::string baseName(const std::string& path) {
    size_t slash = path.find_last_of("/\\");
    return slash == std::string::npos ? path : path.substr(slash + 1);
}

/// Canonical path of the file a location was written in, or "" for library code.
std::string fileOf(const slang::SourceManager& sm, slang::SourceLocation loc) {
    if (!meth::isUserLocation(sm, loc)) return {};
    loc = sm.getFullyOriginalLoc(loc);
    return history::canonicalPath(sm.getFullPath(loc.buffer()).string());
}

size_t maxCommits(std::string_view ruleId) {
    return static_cast<size_t>(std::max<int64_t>(1, swe::ruleOption(ruleId, "max_commits", 1000)));
}

} // namespace

class Hotspot : public Rule {
public:
    std::string_view getId() const override { return "SV-EVO-001"; }
    std::string_view getName() const override { return "Hotspot"; }
    std::string_view getDescription() const override {
        return "With --history: reports the files that are both complex (summed cyclomatic "
               "complexity of their routines) and often changed in git, the top five by complexity "
               "times commits. Such hotspots are where defects collect and where refactoring pays "
               "first (Tornhill, MG-9.2). Advisory: off by default.";
    }
    std::string_view getCategory() const override { return "Advisory"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Info; }

    void run(Compilation& comp, const uvm::UvmModel& model, core::DiagnosticEngine& diags) override {
        const auto* sm = comp.getSourceManager();
        if (!sm || !history::enabled()) return;
        int64_t topFiles = swe::ruleOption("SV-EVO-001", "top_files", 5);
        int64_t minCommits = swe::ruleOption("SV-EVO-001", "min_commits", 3);
        int64_t minComplexity = swe::ruleOption("SV-EVO-001", "min_complexity", 15);
        size_t commits = maxCommits("SV-EVO-001");

        struct FileStats {
            int complexity = 0;
            int worst = 0;
            const SubroutineSymbol* worstRoutine = nullptr;
            size_t churn = 0;
            size_t logSize = 0;
        };
        std::map<std::string, FileStats> files;
        for (const auto* sub : swe::allUserRoutines(comp, model)) {
            std::string file = fileOf(*sm, sub->location);
            if (file.empty()) continue;
            int cc = metrics::computeRoutineMetrics(*sub).cyclomatic;
            auto& f = files[file];
            f.complexity += cc;
            if (!f.worstRoutine || cc > f.worst) {
                f.worst = cc;
                f.worstRoutine = sub;
            }
        }

        std::vector<std::pair<int64_t, std::string>> ranked;
        for (auto& [file, f] : files) {
            const auto* log = history::logFor(file, commits);
            if (!log) continue;
            auto it = log->files.find(file);
            f.churn = it == log->files.end() ? 0 : it->second.size();
            f.logSize = log->commitSizes.size();
            if (int64_t(f.churn) < minCommits || f.complexity < minComplexity) continue;
            ranked.push_back({int64_t(f.churn) * f.complexity, file});
        }
        std::sort(ranked.begin(), ranked.end(), [](const auto& a, const auto& b) {
            return a.first != b.first ? a.first > b.first : a.second < b.second;
        });
        for (size_t i = 0; i < ranked.size() && int64_t(i) < topFiles; i++) {
            const auto& f = files[ranked[i].second];
            diags.addViolation(
                "SV-EVO-001", "Hotspot",
                "Hotspot #" + std::to_string(i + 1) + ": '" + baseName(ranked[i].second) + "' changed in " +
                    std::to_string(f.churn) + " of the last " + std::to_string(f.logSize) +
                    " commits and its routines add up to cyclomatic complexity " + std::to_string(f.complexity) +
                    "; '" + std::string(f.worstRoutine->name) + "' (" + std::to_string(f.worst) +
                    ") is the most complex. Complex code that keeps changing is where defects collect; "
                    "simplify or split it before adding to it.",
                core::Severity::Info, f.worstRoutine->location);
        }
    }
};

class ChangeCoupling : public Rule {
public:
    std::string_view getId() const override { return "SV-EVO-002"; }
    std::string_view getName() const override { return "ChangeCoupling"; }
    std::string_view getDescription() const override {
        return "With --history: reports two files in different packages that change together in at "
               "least three commits and in at least half of the commits touching either. The "
               "packages have a hidden dependency that their imports do not show; move the shared "
               "knowledge to one place (temporal coupling, MG-9.2). Advisory: off by default.";
    }
    std::string_view getCategory() const override { return "Advisory"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Info; }

    void run(Compilation& comp, const uvm::UvmModel& model, core::DiagnosticEngine& diags) override {
        const auto* sm = comp.getSourceManager();
        if (!sm || !history::enabled()) return;
        int64_t minShared = swe::ruleOption("SV-EVO-002", "min_shared_commits", 3);
        int64_t minDegree = swe::ruleOption("SV-EVO-002", "min_degree", 50);
        size_t maxFiles = static_cast<size_t>(swe::ruleOption("SV-EVO-002", "max_commit_files", 30));
        size_t commits = maxCommits("SV-EVO-002");

        struct FileInfo {
            std::set<std::string> packages;
            const ClassType* firstClass = nullptr;
        };
        std::map<std::string, FileInfo> files;
        for (const auto* cls : swe::userClasses(model)) {
            const Scope* s = cls->getParentScope();
            if (!s || s->asSymbol().kind != SymbolKind::Package) continue;
            std::string file = fileOf(*sm, cls->location);
            if (file.empty()) continue;
            auto& info = files[file];
            info.packages.insert(std::string(s->asSymbol().name));
            if (!info.firstClass) info.firstClass = cls;
        }

        // Commits per file, leaving out sweeping commits (reformatting, renames, licence headers)
        // that touch many files at once and say nothing about design.
        struct Revs {
            const history::Log* log = nullptr;
            std::vector<uint32_t> commits;
        };
        std::vector<std::pair<const std::string*, Revs>> revs;
        for (const auto& [file, info] : files) {
            const auto* log = history::logFor(file, commits);
            if (!log) continue;
            auto it = log->files.find(file);
            if (it == log->files.end()) continue;
            Revs r{log, {}};
            for (uint32_t c : it->second) {
                if (log->commitSizes[c] <= maxFiles) r.commits.push_back(c);
            }
            if (int64_t(r.commits.size()) >= minShared) revs.push_back({&file, std::move(r)});
        }

        for (size_t i = 0; i < revs.size(); i++) {
            for (size_t j = i + 1; j < revs.size(); j++) {
                const auto& [fa, ra] = revs[i];
                const auto& [fb, rb] = revs[j];
                if (ra.log != rb.log) continue;
                const auto& pa = files[*fa].packages;
                const auto& pb = files[*fb].packages;
                std::vector<std::string> common;
                std::set_intersection(pa.begin(), pa.end(), pb.begin(), pb.end(), std::back_inserter(common));
                if (!common.empty()) continue;
                // Commit indices are newest first, so both lists are sorted ascending.
                std::vector<uint32_t> shared;
                std::set_intersection(ra.commits.begin(), ra.commits.end(), rb.commits.begin(), rb.commits.end(),
                                      std::back_inserter(shared));
                int64_t n = int64_t(shared.size());
                int64_t total = int64_t(ra.commits.size() + rb.commits.size());
                if (n < minShared || n * 200 < minDegree * total) continue;
                diags.addViolation(
                    "SV-EVO-002", "ChangeCoupling",
                    "'" + baseName(*fb) + "' (package " + *pb.begin() + ") changed together with '" + baseName(*fa) +
                        "' (package " + *pa.begin() + ") in " + std::to_string(n) + " commits, " +
                        std::to_string(n * 200 / total) + "% of the commits touching either. Packages "
                        "that keep changing together share knowledge their imports do not show; move it "
                        "to one place or make the dependency explicit.",
                    core::Severity::Info, files[*fb].firstClass->location);
            }
        }
    }
};

} // namespace uvmcheck::rules::methodology

namespace uvmcheck::rules {
std::unique_ptr<Rule> createHotspot() { return std::make_unique<methodology::Hotspot>(); }
std::unique_ptr<Rule> createChangeCoupling() { return std::make_unique<methodology::ChangeCoupling>(); }
}
