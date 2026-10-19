#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"
#include <string>

namespace uvmcheck::rules::deadcode {

class UnusedGenerateVisitor : public slang::syntax::SyntaxVisitor<UnusedGenerateVisitor> {
public:
    explicit UnusedGenerateVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::ConditionalStatementSyntax& stmt) {
        if (stmt.predicate && stmt.elseClause == nullptr) {
            if (stmt.predicate->conditions.size() == 1) {
                const auto* cond = stmt.predicate->conditions[0];
                if (cond->expr) {
                    std::string condStr = std::string(cond->expr->toString());
                    condStr.erase(0, condStr.find_first_not_of(" \t\n\r"));
                    if (size_t end = condStr.find_last_not_of(" \t\n\r"); end != std::string::npos) {
                        condStr.erase(end + 1);
                    }

                    if (condStr == "0" || condStr == "1'b0" || condStr == "32'd0" || condStr == "false") {
                        diags.addViolation(
                            "SV-DED-008",
                            "UnusedGenerateBlock",
                            "Conditional statement specifies compile-time false condition '" + condStr +
                            "' with no 'else' fallback. The enclosed code can never be reached or executed under any simulation conditions.",
                            core::Severity::Info,
                            cond->expr->getFirstToken().location()
                        );
                    }
                }
            }
        }

        visitDefault(stmt);
    }

private:
    core::DiagnosticEngine& diags;
};

class UnusedGenerateBlock : public Rule {
public:
    std::string_view getId() const override { return "SV-DED-008"; }
    std::string_view getName() const override { return "UnusedGenerateBlock"; }
    std::string_view getDescription() const override {
        return "Flags conditional statements or generate blocks with constant false conditions ('0') and no 'else' clause.";
    }
    std::string_view getCategory() const override { return "DeadCode"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Info; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& /*uvmModel*/,
             core::DiagnosticEngine& diags) override {
        UnusedGenerateVisitor visitor(diags);
        for (const auto& tree : compilation.getSyntaxTrees()) {
            if (tree) {
                tree->root().visit(visitor);
            }
        }
    }
};

} // namespace uvmcheck::rules::deadcode

namespace uvmcheck::rules {
std::unique_ptr<Rule> createUnusedGenerateBlock() {
    return std::make_unique<deadcode::UnusedGenerateBlock>();
}
}
