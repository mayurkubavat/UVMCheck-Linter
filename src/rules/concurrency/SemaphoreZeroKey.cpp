#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"
#include <string>

namespace uvmcheck::rules::concurrency {

class SemaphoreZeroKeyVisitor : public slang::syntax::SyntaxVisitor<SemaphoreZeroKeyVisitor> {
public:
    explicit SemaphoreZeroKeyVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::InvocationExpressionSyntax& inv) {
        if (inv.left) {
            std::string methodName = std::string(inv.left->getLastToken().valueText());
            if (methodName == "get" || methodName == "put" || methodName == "try_get") {
                std::string callStr = std::string(inv.left->toString());
                if (callStr.find("sem") != std::string::npos) {
                    if (inv.arguments && !inv.arguments->parameters.empty()) {
                        const auto* firstArg = inv.arguments->parameters[0];
                        if (firstArg->kind == slang::syntax::SyntaxKind::OrderedArgument) {
                            const auto& ordArg = firstArg->as<slang::syntax::OrderedArgumentSyntax>();
                            std::string argStr = std::string(ordArg.expr->toString());
                            argStr.erase(0, argStr.find_first_not_of(" \t\n\r"));
                            if (size_t end = argStr.find_last_not_of(" \t\n\r"); end != std::string::npos) {
                                argStr.erase(end + 1);
                            }

                            if (argStr == "0" || argStr == "32'd0" || argStr == "1'b0" ||
                                argStr.starts_with("-")) {
                                diags.addViolation(
                                    "SV-CON-005",
                                    "SemaphoreZeroKey",
                                    "Semaphore operation '." + methodName + "(" + argStr +
                                    ")' specifies a non-positive key count (<= 0). Per IEEE 1800 § 15.3, key counts must be positive integers >= 1. Requesting 0 keys causes immediate simulation deadlock or undefined simulator scheduling behavior.",
                                    core::Severity::Error,
                                    ordArg.expr->getFirstToken().location()
                                );
                            }
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

class SemaphoreZeroKey : public Rule {
public:
    std::string_view getId() const override { return "SV-CON-005"; }
    std::string_view getName() const override { return "SemaphoreZeroKey"; }
    std::string_view getDescription() const override {
        return "Flags semaphore method calls (sem.get(0), sem.put(0), sem.try_get(0)) passing non-positive key counts (<= 0).";
    }
    std::string_view getCategory() const override { return "Concurrency"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& /*uvmModel*/,
             core::DiagnosticEngine& diags) override {
        SemaphoreZeroKeyVisitor visitor(diags);
        for (const auto& tree : compilation.getSyntaxTrees()) {
            if (tree) {
                tree->root().visit(visitor);
            }
        }
    }
};

} // namespace uvmcheck::rules::concurrency

namespace uvmcheck::rules {
std::unique_ptr<Rule> createSemaphoreZeroKey() {
    return std::make_unique<concurrency::SemaphoreZeroKey>();
}
}
