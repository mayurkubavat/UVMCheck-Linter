#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"
#include <string>

namespace uvmcheck::rules::style {

class UvmTlmPortSuffixVisitor : public slang::syntax::SyntaxVisitor<UvmTlmPortSuffixVisitor> {
public:
    explicit UvmTlmPortSuffixVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::DataDeclarationSyntax& decl) {
        if (!decl.type) {
            visitDefault(decl);
            return;
        }

        std::string baseType = std::string(decl.type->getFirstToken().valueText());

        std::string expectedSuffix;
        std::string ruleDesc;

        if (baseType == "uvm_analysis_port") {
            expectedSuffix = "_ap";
            ruleDesc = "'_ap' or '_port'";
        } else if (baseType.find("analysis_imp") != std::string::npos || baseType.ends_with("_imp")) {
            expectedSuffix = "_imp";
            ruleDesc = "'_imp'";
        } else if (baseType.find("analysis_export") != std::string::npos || baseType.ends_with("_export")) {
            expectedSuffix = "_export";
            ruleDesc = "'_export' or '_exp'";
        } else if (baseType.find("tlm_fifo") != std::string::npos || baseType.ends_with("_fifo")) {
            expectedSuffix = "_fifo";
            ruleDesc = "'_fifo'";
        } else if (baseType.find("uvm_") != std::string::npos && baseType.ends_with("_port")) {
            expectedSuffix = "_port";
            ruleDesc = "'_port'";
        }

        if (!expectedSuffix.empty()) {
            for (const auto* declVar : decl.declarators) {
                if (!declVar) continue;
                std::string varName = std::string(declVar->name.valueText());
                if (varName.empty()) continue;

                bool matches = false;
                if (expectedSuffix == "_ap") {
                    matches = varName.ends_with("_ap") || varName.ends_with("_port") || varName == "ap";
                } else if (expectedSuffix == "_export") {
                    matches = varName.ends_with("_export") || varName.ends_with("_exp") || varName == "export" || varName == "exp";
                } else if (expectedSuffix == "_fifo") {
                    matches = varName.ends_with("_fifo") || varName == "fifo";
                } else if (expectedSuffix == "_imp") {
                    matches = varName.ends_with("_imp") || varName == "imp";
                } else {
                    matches = varName.ends_with(expectedSuffix);
                }

                if (!matches) {
                    diags.addViolation(
                        "UVM-TLM-003",
                        "UvmTlmPortSuffix",
                        "TLM object '" + varName + "' of type '" + baseType + "' violates naming conventions: "
                        "expected suffix " + ruleDesc + ". "
                        "Standardized TLM suffixes prevent confusion between ports, exports, and imps during connect_phase.",
                        core::Severity::Warning,
                        declVar->name.location()
                    );
                }
            }
        }

        visitDefault(decl);
    }

private:
    core::DiagnosticEngine& diags;
};

class UvmTlmPortSuffix : public Rule {
public:
    std::string_view getId() const override { return "UVM-TLM-003"; }
    std::string_view getName() const override { return "UvmTlmPortSuffix"; }
    std::string_view getDescription() const override {
        return "Enforces standard UVM TLM port suffix naming conventions (_ap for analysis ports, _imp for imps, "
               "_export for exports, _port for ports, _fifo for TLM FIFOs).";
    }
    std::string_view getCategory() const override { return "Style"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& /*uvmModel*/,
             core::DiagnosticEngine& diags) override {
        UvmTlmPortSuffixVisitor visitor(diags);
        for (const auto& tree : compilation.getSyntaxTrees()) {
            if (tree) {
                tree->root().visit(visitor);
            }
        }
    }
};

} // namespace uvmcheck::rules::style

namespace uvmcheck::rules {
std::unique_ptr<Rule> createUvmTlmPortSuffix() {
    return std::make_unique<style::UvmTlmPortSuffix>();
}
}
