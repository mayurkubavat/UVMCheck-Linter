#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"
#include <algorithm>
#include <cctype>

namespace uvmcheck::rules::uvm {

class ConfigDbWildcardVisitor : public slang::syntax::SyntaxVisitor<ConfigDbWildcardVisitor> {
public:
    explicit ConfigDbWildcardVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::InvocationExpressionSyntax& inv) {
        if (inv.left) {
            std::string callee = std::string(inv.left->toString());
            // Check if call is uvm_config_db...::set
            if (callee.find("uvm_config_db") != std::string::npos && callee.find("::set") != std::string::npos) {
                if (inv.arguments && inv.arguments->parameters.size() >= 2) {
                    const auto* arg0 = inv.arguments->parameters[0];
                    const auto* arg1 = inv.arguments->parameters[1];

                    std::string str0 = arg0 ? std::string(arg0->toString()) : "";
                    str0.erase(0, str0.find_first_not_of(" \t\n\r"));
                    if (size_t end = str0.find_last_not_of(" \t\n\r"); end != std::string::npos) {
                        str0.erase(end + 1);
                    }

                    std::string str1 = arg1 ? std::string(arg1->toString()) : "";
                    str1.erase(0, str1.find_first_not_of(" \t\n\r"));
                    if (size_t end = str1.find_last_not_of(" \t\n\r"); end != std::string::npos) {
                        str1.erase(end + 1);
                    }

                    if (str0 == "null" && (str1 == "\"*\"" || str1 == "\"\"")) {
                        diags.addViolation(
                            "UVM-CFG-003",
                            "UvmConfigDbGlobalWildcard",
                            "Unbounded global config_db scope: uvm_config_db::set(null, " + str1 +
                            ", ...) sets configuration globally with no hierarchical context. "
                            "This forces every component in the testbench hierarchy to perform regex string matching during build_phase, severely degrading simulation startup performance. Provide targeted scope (e.g. 'uvm_test_top.env.*').",
                            core::Severity::Warning,
                            inv.sourceRange().start()
                        );
                    }
                }
            }
        }
        visitDefault(inv);
    }

private:
    core::DiagnosticEngine& diags;
};

class UvmConfigDbGlobalWildcard : public Rule {
public:
    std::string_view getId() const override { return "UVM-CFG-003"; }
    std::string_view getName() const override { return "UvmConfigDbGlobalWildcard"; }
    std::string_view getDescription() const override {
        return "Calling uvm_config_db::set(null, \"*\", ...) with a null context and a bare wildcard or empty "
               "scope triggers global regex matching across every component in the testbench, severely degrading simulation startup.";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& /*uvmModel*/,
             core::DiagnosticEngine& diags) override {
        ConfigDbWildcardVisitor visitor(diags);
        for (const auto& tree : compilation.getSyntaxTrees()) {
            if (tree) {
                tree->root().visit(visitor);
            }
        }
    }
};

} // namespace uvmcheck::rules::uvm

namespace uvmcheck::rules {
std::unique_ptr<Rule> createUvmConfigDbGlobalWildcard() {
    return std::make_unique<uvm::UvmConfigDbGlobalWildcard>();
}
}
