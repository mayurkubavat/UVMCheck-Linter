#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"
#include <string>

namespace uvmcheck::rules::ral {

class BackdoorPathVisitor : public engine::SyntaxVisitor<BackdoorPathVisitor> {
public:
    explicit BackdoorPathVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::InvocationExpressionSyntax& inv) {
        if (inv.left) {
            std::string methodName = std::string(inv.left->getLastToken().valueText());
            if (methodName == "add_hdl_path" || methodName == "set_hdl_path_root" ||
                methodName == "add_hdl_path_slice" || methodName == "set_default_hdl_path") {
                if (inv.arguments && !inv.arguments->parameters.empty()) {
                    const auto* firstArg = inv.arguments->parameters[0];
                    if (firstArg->kind == slang::syntax::SyntaxKind::OrderedArgument) {
                        const auto& ord = firstArg->as<slang::syntax::OrderedArgumentSyntax>();
                        std::string pathStr = std::string(ord.expr->toString());
                        pathStr.erase(0, pathStr.find_first_not_of(" \t\n\r"));
                        if (size_t end = pathStr.find_last_not_of(" \t\n\r"); end != std::string::npos) {
                            pathStr.erase(end + 1);
                        }

                        if (pathStr == "\"\"" || pathStr == "null") {
                            diags.addViolation(
                                "RAL-ACC-002",
                                "MissingBackdoorAccessPathDefinition",
                                "RAL backdoor path configuration '" + methodName + "(\"\")' specifies an empty or null HDL path string. "
                                "In UVM RAL architecture (IEEE 1800.2 § 18.8), register backdoor read/write operations "
                                "require a non-empty HDL root path to resolve target register signals within the simulation hierarchy.",
                                core::Severity::Warning,
                                ord.expr->getFirstToken().location()
                            );
                        }
                    }
                }
            }
        }
        visitDefault(inv);
    }

private:
    core::DiagnosticEngine& diags;
};

class MissingBackdoorAccessPathDefinition : public Rule {
public:
    std::string_view getId() const override { return "RAL-ACC-002"; }
    std::string_view getName() const override { return "MissingBackdoorAccessPathDefinition"; }
    std::string_view getDescription() const override {
        return "Flags RAL backdoor path configuration routines called with an empty string or null path.";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        auto& diags = ctx.diags();
        BackdoorPathVisitor visitor(diags);
        co_await ctx.visitSyntax(visitor);
    }
};

} // namespace uvmcheck::rules::ral

namespace uvmcheck::rules {
std::unique_ptr<Rule> createMissingBackdoorAccessPathDefinition() {
    return std::make_unique<ral::MissingBackdoorAccessPathDefinition>();
}
} // namespace uvmcheck::rules
