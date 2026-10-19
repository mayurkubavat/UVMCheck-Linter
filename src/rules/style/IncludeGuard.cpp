#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/AllSyntax.h"

namespace uvmcheck::rules::style {

class IncludeGuard : public Rule {
public:
    std::string_view getId() const override { return "SV-STY-013"; }
    std::string_view getName() const override { return "IncludeGuard"; }
    std::string_view getDescription() const override {
        return "Every included header file (.svh) must be wrapped in standard macro include guards "
               "(`ifndef / `define / `endif) to prevent circular definitions and compiler redefinition errors.";
    }
    std::string_view getCategory() const override { return "Preprocessor"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel&,
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

                bool hasIfndef = (sourceText.find("`ifndef") != std::string_view::npos);
                bool hasDefine = (sourceText.find("`define") != std::string_view::npos);
                bool hasEndif  = (sourceText.find("`endif") != std::string_view::npos);

                if (!hasIfndef || !hasDefine || !hasEndif) {
                    diags.addViolation(
                        "SV-STY-013",
                        "IncludeGuard",
                        "Header file '" + filePath +
                        "' is missing standard macro include guards (`ifndef / `define / `endif). "
                        "By convention, all header files (.svh) must be protected by include guards to prevent multiple inclusion errors.",
                        core::Severity::Error,
                        firstToken.location()
                    );
                }
            }
        }
    }
};

} // namespace uvmcheck::rules::style

namespace uvmcheck::rules {
std::unique_ptr<Rule> createIncludeGuard() {
    return std::make_unique<style::IncludeGuard>();
}
}
