#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"
#include <string>

namespace uvmcheck::rules::uvm {

class ComponentNullParentVisitor : public engine::SyntaxVisitor<ComponentNullParentVisitor> {
public:
    explicit ComponentNullParentVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::InvocationExpressionSyntax& inv) {
        if (inv.left) {
            std::string invLeft = std::string(inv.left->toString());
            // Look for ::type_id::create(...)
            if (invLeft.find("::type_id::create") != std::string::npos ||
                invLeft.find(".create") != std::string::npos) {
                if (inv.arguments && inv.arguments->parameters.size() >= 2) {
                    const auto* parentArg = inv.arguments->parameters[1];
                    if (parentArg->kind == slang::syntax::SyntaxKind::OrderedArgument) {
                        const auto& ord = parentArg->as<slang::syntax::OrderedArgumentSyntax>();
                        std::string parentStr = std::string(ord.expr->toString());
                        parentStr.erase(0, parentStr.find_first_not_of(" \t\n\r"));
                        if (size_t end = parentStr.find_last_not_of(" \t\n\r"); end != std::string::npos) {
                            parentStr.erase(end + 1);
                        }

                        if (parentStr == "null") {
                            std::string callClean = std::string(inv.toString());
                            if (size_t lastNl = callClean.rfind('\n'); lastNl != std::string::npos) {
                                callClean = callClean.substr(lastNl + 1);
                            }
                            callClean.erase(0, callClean.find_first_not_of(" \t\r"));
                            if (size_t end = callClean.find_last_not_of(" \t\r"); end != std::string::npos) {
                                callClean.erase(end + 1);
                            }

                            diags.addViolation(
                                "UVM-STR-005",
                                "ComponentNullParentInCreate",
                                "Component factory creation '" + callClean +
                                "' passes 'null' as the parent component handle. In UVM methodology (IEEE 1800.2 § 4.3 and "
                                "IEEE 1800.2 § 4.3), sub-components must specify 'this' as parent to properly integrate into "
                                "the component hierarchy. Passing 'null' creates an orphaned component excluded from phase "
                                "traversal, topology reporting, and hierarchical uvm_config_db lookup.",
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

class ComponentNullParentInCreate : public Rule {
public:
    std::string_view getId() const override { return "UVM-STR-005"; }
    std::string_view getName() const override { return "ComponentNullParentInCreate"; }
    std::string_view getDescription() const override {
        return "Flags UVM component factory create calls passing 'null' as the parent handle, creating orphaned hierarchy nodes.";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        auto& diags = ctx.diags();
        ComponentNullParentVisitor visitor(diags);
        co_await ctx.visitSyntax(visitor);
    }
};

} // namespace uvmcheck::rules::uvm

namespace uvmcheck::rules {
std::unique_ptr<Rule> createComponentNullParentInCreate() {
    return std::make_unique<uvm::ComponentNullParentInCreate>();
}
} // namespace uvmcheck::rules
