#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"
#include <string>

namespace uvmcheck::rules::types {

class TaskReturnTypeVisitor : public engine::SyntaxVisitor<TaskReturnTypeVisitor> {
public:
    explicit TaskReturnTypeVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::FunctionDeclarationSyntax& decl) {
        if (decl.kind == slang::syntax::SyntaxKind::TaskDeclaration) {
            if (decl.prototype && decl.prototype->returnType) {
                std::string retStr = std::string(decl.prototype->returnType->toString());
                retStr.erase(0, retStr.find_first_not_of(" \t\n\r"));
                if (size_t end = retStr.find_last_not_of(" \t\n\r"); end != std::string::npos) {
                    retStr.erase(end + 1);
                }

                if (!retStr.empty() && retStr != "void") {
                    std::string taskName = std::string(decl.prototype->name ? decl.prototype->name->toString() : "task");
                    taskName.erase(0, taskName.find_first_not_of(" \t\n\r"));
                    if (size_t end = taskName.find_last_not_of(" \t\n\r"); end != std::string::npos) {
                        taskName.erase(end + 1);
                    }

                    diags.addViolation(
                        "SV-TYP-010",
                        "TaskWithNonVoidReturnType",
                        "Task '" + taskName + "' is declared with non-void return type '" + retStr +
                        "'. In SystemVerilog (IEEE 1800 § 13.3), tasks cannot return values "
                        "and cannot specify a return type. Use a 'function' if a return value is required, or remove the return type.",
                        core::Severity::Error,
                        decl.prototype->returnType->getFirstToken().location()
                    );
                }
            }
        }
        visitDefault(decl);
    }

private:
    core::DiagnosticEngine& diags;
};

class TaskWithNonVoidReturnType : public Rule {
public:
    std::string_view getId() const override { return "SV-TYP-010"; }
    std::string_view getName() const override { return "TaskWithNonVoidReturnType"; }
    std::string_view getDescription() const override {
        return "Disallows declaring tasks with explicit non-void return types.";
    }
    std::string_view getCategory() const override { return "Types"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        auto& diags = ctx.diags();
        TaskReturnTypeVisitor visitor(diags);
        co_await ctx.visitSyntax(visitor);
    }
};

} // namespace uvmcheck::rules::types

namespace uvmcheck::rules {
std::unique_ptr<Rule> createTaskWithNonVoidReturnType() {
    return std::make_unique<types::TaskWithNonVoidReturnType>();
}
} // namespace uvmcheck::rules
