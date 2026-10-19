#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/SyntaxTree.h"
#include <unordered_set>
#include <vector>

namespace uvmcheck::rules::portability {

class HeaderMacroLeak : public Rule {
public:
    std::string_view getId() const override { return "SV-MAC-001"; }
    std::string_view getName() const override { return "HeaderMacroLeak"; }
    std::string_view getDescription() const override {
        return "Macros defined inside header files (.svh) must be undef'd with `undef before the end of the file "
               "unless registered as public API macros. Leaking macros causes silent parameter overrides and compilation collisions.";
    }
    std::string_view getCategory() const override { return "Portability"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }
    uint32_t getProfiles() const override {
        return static_cast<uint32_t>(RuleProfile::Methodology) | static_cast<uint32_t>(RuleProfile::Style);
    }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& /*uvmModel*/,
             core::DiagnosticEngine& diags) override {
        const auto* sm = compilation.getSourceManager();
        if (!sm) return;

        for (const auto& tree : compilation.getSyntaxTrees()) {
            auto firstToken = tree->root().getFirstToken();
            if (firstToken.isMissing()) continue;

            auto buffer = firstToken.location().buffer();
            std::string filePath = std::string(sm->getFullPath(buffer));

            if (filePath.ends_with(".svh")) {
                std::string_view sourceText = sm->getSourceText(buffer);

                std::vector<std::pair<std::string, size_t>> definedMacros;
                std::unordered_set<std::string> undefedMacros;

                size_t pos = 0;
                while ((pos = sourceText.find("`define", pos)) != std::string_view::npos) {
                    if (pos == 0 || sourceText[pos - 1] == '\n' || sourceText[pos - 1] == ' ' || sourceText[pos - 1] == '\t') {
                        size_t nameStart = sourceText.find_first_not_of(" \t", pos + 7);
                        if (nameStart != std::string_view::npos) {
                            size_t nameEnd = sourceText.find_first_of(" \t\r\n(", nameStart);
                            if (nameEnd == std::string_view::npos) nameEnd = sourceText.size();
                            std::string macroName = std::string(sourceText.substr(nameStart, nameEnd - nameStart));

                            bool isGuard = macroName.ends_with("_SVH") || macroName.ends_with("_H") ||
                                           macroName.ends_with("_INC") || macroName.ends_with("_GUARD");
                            bool isPublicApi = macroName.starts_with("uvm_") || macroName.starts_with("UVM_");

                            if (!isGuard && !isPublicApi && !macroName.empty()) {
                                definedMacros.push_back({macroName, nameStart});
                            }
                        }
                    }
                    pos += 7;
                }

                pos = 0;
                while ((pos = sourceText.find("`undef", pos)) != std::string_view::npos) {
                    if (pos == 0 || sourceText[pos - 1] == '\n' || sourceText[pos - 1] == ' ' || sourceText[pos - 1] == '\t') {
                        size_t nameStart = sourceText.find_first_not_of(" \t", pos + 6);
                        if (nameStart != std::string_view::npos) {
                            size_t nameEnd = sourceText.find_first_of(" \t\r\n", nameStart);
                            if (nameEnd == std::string_view::npos) nameEnd = sourceText.size();
                            std::string macroName = std::string(sourceText.substr(nameStart, nameEnd - nameStart));
                            undefedMacros.insert(macroName);
                        }
                    }
                    pos += 6;
                }

                for (const auto& [macroName, offset] : definedMacros) {
                    if (undefedMacros.count(macroName) == 0) {
                        slang::SourceLocation loc(buffer, offset);
                        diags.addViolation(
                            "SV-MAC-001",
                            "HeaderMacroLeak",
                            "Macro `" + macroName + " is defined in header file '" + filePath +
                            "' without a corresponding `undef before file end. "
                            "Leaking preprocessor definitions into global compilation scope causes accidental macro collisions across packages.",
                            core::Severity::Warning,
                            loc
                        );
                    }
                }
            }
        }
    }
};

} // namespace uvmcheck::rules::portability

namespace uvmcheck::rules {
std::unique_ptr<Rule> createHeaderMacroLeak() {
    return std::make_unique<portability::HeaderMacroLeak>();
}
}
