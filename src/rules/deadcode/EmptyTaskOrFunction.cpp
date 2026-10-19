#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"

namespace uvmcheck::rules::deadcode {

class EmptyRoutineVisitor : public slang::syntax::SyntaxVisitor<EmptyRoutineVisitor> {
public:
    explicit EmptyRoutineVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::FunctionDeclarationSyntax& func) {
        if (!func.prototype || !func.prototype->name) {
            visitDefault(func);
            return;
        }

        // Skip prototypes without an 'endtask'/'endfunction'
        if (func.end.kind == slang::parsing::TokenKind::Unknown) {
            visitDefault(func);
            return;
        }

        // Check if extern, pure, or virtual
        bool isExtern = false;
        bool isPure = false;
        bool isVirtual = false;

        for (const auto* s : func.prototype->specifiers) {
            if (!s) continue;
            if (s->keyword.kind == slang::parsing::TokenKind::PureKeyword) isPure = true;
            if (s->keyword.kind == slang::parsing::TokenKind::VirtualKeyword) isVirtual = true;
        }

        if (func.parent && func.parent->kind == slang::syntax::SyntaxKind::ClassMethodDeclaration) {
            const auto& method = func.parent->as<slang::syntax::ClassMethodDeclarationSyntax>();
            for (const auto& q : method.qualifiers) {
                if (q.kind == slang::parsing::TokenKind::PureKeyword) isPure = true;
                if (q.kind == slang::parsing::TokenKind::VirtualKeyword) isVirtual = true;
                if (q.kind == slang::parsing::TokenKind::ExternKeyword) isExtern = true;
            }
        }

        // Skip prototypes, pure virtual, virtual (handled by SV-OOP-009), or extern
        if (isExtern || isPure || isVirtual) {
            visitDefault(func);
            return;
        }

        std::string routineName = std::string(func.prototype->name->toString());
        routineName.erase(0, routineName.find_first_not_of(" \t\n\r"));
        if (size_t end = routineName.find_last_not_of(" \t\n\r"); end != std::string::npos) {
            routineName.erase(end + 1);
        }

        // Skip sequence body (handled by UVM-SEQ-005) and constructor new
        if (routineName == "body" || routineName == "new") {
            visitDefault(func);
            return;
        }

        bool hasStmt = false;
        for (const auto* item : func.items) {
            if (!item) continue;
            if (item->kind != slang::syntax::SyntaxKind::EmptyStatement) {
                hasStmt = true;
                break;
            }
        }

        if (!hasStmt) {
            std::string routineKind = (func.prototype->keyword.kind == slang::parsing::TokenKind::TaskKeyword)
                                      ? "Task" : "Function";
            diags.addViolation(
                "SV-DED-011",
                "EmptyTaskOrFunction",
                routineKind + " '" + routineName + "' has an empty body with zero executable statements. "
                "Concrete routines with empty bodies execute as no-ops, consume simulation call-stack overhead, "
                "and typically represent unfinished testbench scaffolding or forgotten implementation stubs.",
                core::Severity::Warning,
                func.prototype->name->sourceRange().start()
            );
        }

        visitDefault(func);
    }

private:
    core::DiagnosticEngine& diags;
};

class EmptyTaskOrFunction : public Rule {
public:
    std::string_view getId() const override { return "SV-DED-011"; }
    std::string_view getName() const override { return "EmptyTaskOrFunction"; }
    std::string_view getDescription() const override {
        return "Flags concrete, non-virtual, non-extern tasks and functions with empty bodies (zero statements). "
               "Empty routines cause useless call overhead and indicate incomplete or dead logic.";
    }
    std::string_view getCategory() const override { return "DeadCode"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& /*uvmModel*/,
             core::DiagnosticEngine& diags) override {
        EmptyRoutineVisitor visitor(diags);
        for (const auto& tree : compilation.getSyntaxTrees()) {
            if (tree) {
                tree->root().visit(visitor);
            }
        }
    }
};

} // namespace uvmcheck::rules::deadcode

namespace uvmcheck::rules {
std::unique_ptr<Rule> createEmptyTaskOrFunction() {
    return std::make_unique<deadcode::EmptyTaskOrFunction>();
}
}
