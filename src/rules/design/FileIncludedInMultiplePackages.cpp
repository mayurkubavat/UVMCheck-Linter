#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/rules/SweSupport.h"
#include "slang/ast/symbols/CompilationUnitSymbols.h"
#include "slang/text/SourceManager.h"

#include <map>
#include <set>

namespace uvmcheck::rules::design {

using namespace slang::ast;

class FileIncludedInMultiplePackages : public Rule {
public:
    std::string_view getId() const override { return "UVM-ARC-003"; }
    std::string_view getName() const override { return "FileIncludedInMultiplePackages"; }
    std::string_view getDescription() const override {
        return "Flags a file whose class definitions are `include'd into more than one package. Each "
               "package gets its own, incompatible type with the same name (C++ one-definition rule).";
    }
    std::string_view getCategory() const override { return "Architecture"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    void run(Compilation& compilation, const uvm::UvmModel&, core::DiagnosticEngine& diags) override {
        const auto* sm = compilation.getSourceManager();
        if (!sm) return;

        struct Site { const PackageSymbol* pkg; const Symbol* cls; };
        std::map<std::string, std::vector<Site>> byFile;
        for (const auto* pkg : compilation.getPackages()) {
            if (!pkg || pkg->name == "std" || swe::isLibrarySymbol(*pkg)) continue;
            std::string pkgFile(sm->getFileName(sm->getFullyOriginalLoc(pkg->location)));
            for (const auto& member : pkg->members()) {
                if (member.kind != SymbolKind::ClassType && member.kind != SymbolKind::GenericClassDef) continue;
                std::string file(sm->getFileName(sm->getFullyOriginalLoc(member.location)));
                if (file.empty() || file == pkgFile) continue; // defined inline, not included
                auto& sites = byFile[file];
                bool seenPkg = false;
                for (const auto& s : sites) seenPkg |= s.pkg == pkg;
                if (!seenPkg) sites.push_back({pkg, &member});
            }
        }

        for (const auto& [file, sites] : byFile) {
            if (sites.size() < 2) continue;
            std::string pkgs;
            for (const auto& s : sites) {
                if (!pkgs.empty()) pkgs += ", ";
                pkgs += std::string(s.pkg->name);
            }
            for (size_t i = 1; i < sites.size(); i++) {
                diags.addViolation(
                    "UVM-ARC-003", "FileIncludedInMultiplePackages",
                    "'" + std::string(sites[i].cls->name) + "' is compiled into packages " + pkgs +
                        " because its file is `include'd into each. That creates distinct types with the "
                        "same name, so $cast, config_db and TLM connections between them fail at run time. "
                        "Include it in one package and import that package elsewhere.",
                    core::Severity::Error, sites[i].pkg->location);
            }
        }
    }
};

} // namespace uvmcheck::rules::design

namespace uvmcheck::rules {
std::unique_ptr<Rule> createFileIncludedInMultiplePackages() {
    return std::make_unique<design::FileIncludedInMultiplePackages>();
}
}
